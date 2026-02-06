class RedisSubscriberJob < ApplicationJob
  queue_as :default

  # This job subscribes to the Redis "bankruptcy:events" channel
  # and processes incoming events from the Rust engine.
  #
  # The Rust engine publishes JSON events to this channel whenever
  # it detects a new bankruptcy filing.

  REDIS_CHANNEL = "bankruptcy:events".freeze

  def perform
    Rails.logger.info "[DOOM TRACKER] Redis subscriber starting on channel: #{REDIS_CHANNEL}"

    redis = Redis.new(url: ENV.fetch("REDIS_URL", "redis://localhost:6379/1"))

    redis.subscribe(REDIS_CHANNEL) do |on|
      on.subscribe do |channel, subscriptions|
        Rails.logger.info "[DOOM TRACKER] Subscribed to #{channel} (#{subscriptions} active subscriptions)"
      end

      on.message do |channel, message|
        Rails.logger.info "[DOOM TRACKER] Received message on #{channel}"
        process_event(message)
      end

      on.unsubscribe do |channel, subscriptions|
        Rails.logger.info "[DOOM TRACKER] Unsubscribed from #{channel} (#{subscriptions} remaining)"
      end
    end
  rescue Redis::BaseConnectionError => e
    Rails.logger.error "[DOOM TRACKER] Redis connection error: #{e.message}. Retrying in 5 seconds..."
    sleep 5
    retry
  rescue StandardError => e
    Rails.logger.error "[DOOM TRACKER] Redis subscriber error: #{e.message}"
    Rails.logger.error e.backtrace.first(10).join("\n")
    sleep 5
    retry
  end

  private

  def process_event(raw_message)
    data = JSON.parse(raw_message)
    Rails.logger.info "[DOOM TRACKER] Processing event: #{data['company_name'] || data['companyName'] || 'unknown'}"

    # Normalize keys from Rust (snake_case or camelCase)
    event = BankruptcyEvent.create!(
      company_name:     data["company_name"] || data["companyName"],
      dot_number:       data["dot_number"] || data["dotNumber"],
      mc_number:        data["mc_number"] || data["mcNumber"],
      filing_date:      parse_date(data["filing_date"] || data["filingDate"]),
      court:            data["court"],
      chapter:          parse_chapter(data["chapter"]),
      source:           data["source"] || "rust_engine",
      confidence_score: (data["confidence_score"] || data["confidenceScore"] || 0.0).to_f,
      raw_data:         data,
      detected_at:      parse_timestamp(data["detected_at"] || data["detectedAt"]),
      status:           "new"
    )

    Rails.logger.info "[DOOM TRACKER] Created event ##{event.id}: #{event.company_name} (Chapter #{event.chapter})"

    # The model's after_create_commit callback handles the Turbo Stream broadcast.
    # Also notify via the channel for raw WebSocket clients.
    BankruptcyEventsChannel.broadcast_event(event)

  rescue JSON::ParserError => e
    Rails.logger.error "[DOOM TRACKER] Failed to parse event JSON: #{e.message}"
    Rails.logger.error "[DOOM TRACKER] Raw message: #{raw_message.truncate(500)}"
  rescue ActiveRecord::RecordInvalid => e
    Rails.logger.error "[DOOM TRACKER] Failed to save event: #{e.message}"
  rescue StandardError => e
    Rails.logger.error "[DOOM TRACKER] Error processing event: #{e.message}"
  end

  def parse_date(value)
    return nil if value.blank?
    Date.parse(value.to_s)
  rescue ArgumentError
    nil
  end

  def parse_chapter(value)
    return nil if value.blank?
    ch = value.to_i
    [7, 11, 13, 15].include?(ch) ? ch : nil
  end

  def parse_timestamp(value)
    return Time.current if value.blank?
    Time.parse(value.to_s)
  rescue ArgumentError
    Time.current
  end
end
