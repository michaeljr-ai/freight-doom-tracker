# Start the Redis subscriber in a background thread on server boot.
# This listens for bankruptcy events published by the Rust engine
# on the "bankruptcy:events" Redis channel.
#
# The subscriber runs in its own thread to avoid blocking the main
# Rails process. It automatically reconnects on connection failures.

Rails.application.config.after_initialize do
  # Only start the subscriber in server mode (not in console, rake, etc.)
  if defined?(Rails::Server) || defined?(Puma)
    Thread.new do
      Rails.logger.info "[DOOM TRACKER] Starting Redis subscriber thread..."

      # Give the server a moment to fully initialize
      sleep 3

      loop do
        begin
          redis = Redis.new(url: ENV.fetch("REDIS_URL", "redis://localhost:6379/1"))

          Rails.logger.info "[DOOM TRACKER] Connecting to Redis for pub/sub on bankruptcy:events..."

          redis.subscribe("bankruptcy:events") do |on|
            on.subscribe do |channel, count|
              Rails.logger.info "[DOOM TRACKER] Subscribed to #{channel} (#{count} subscriptions)"
            end

            on.message do |_channel, message|
              begin
                data = JSON.parse(message)
                Rails.logger.info "[DOOM TRACKER] Received event from Rust engine: #{data['company_name'] || data['companyName']}"

                event = BankruptcyEvent.create!(
                  company_name:     data["company_name"] || data["companyName"],
                  dot_number:       data["dot_number"] || data["dotNumber"],
                  mc_number:        data["mc_number"] || data["mcNumber"],
                  filing_date:      safe_parse_date(data["filing_date"] || data["filingDate"]),
                  court:            data["court"],
                  chapter:          safe_parse_chapter(data["chapter"]),
                  source:           data["source"] || "rust_engine",
                  confidence_score: (data["confidence_score"] || data["confidenceScore"] || 0.0).to_f,
                  raw_data:         data,
                  detected_at:      Time.current,
                  status:           "new"
                )

                Rails.logger.info "[DOOM TRACKER] Stored event ##{event.id}: #{event.company_name}"
              rescue StandardError => e
                Rails.logger.error "[DOOM TRACKER] Error processing message: #{e.message}"
              end
            end

            on.unsubscribe do |channel, count|
              Rails.logger.info "[DOOM TRACKER] Unsubscribed from #{channel}"
            end
          end
        rescue Redis::BaseConnectionError => e
          Rails.logger.warn "[DOOM TRACKER] Redis connection lost: #{e.message}. Reconnecting in 5s..."
          sleep 5
        rescue StandardError => e
          Rails.logger.error "[DOOM TRACKER] Redis subscriber error: #{e.class}: #{e.message}"
          sleep 5
        end
      end
    end
  end
end

# Helper methods for the initializer
def safe_parse_date(value)
  return nil if value.blank?
  Date.parse(value.to_s)
rescue ArgumentError
  nil
end

def safe_parse_chapter(value)
  return nil if value.blank?
  ch = value.to_i
  [7, 11, 13, 15].include?(ch) ? ch : nil
end
