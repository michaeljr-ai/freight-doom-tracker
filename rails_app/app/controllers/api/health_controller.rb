module Api
  class HealthController < ActionController::API
    # GET /api/health or /health
    def index
      health = {
        status: "operational",
        service: "freight-doom-tracker",
        version: "1.0.0",
        timestamp: Time.current.iso8601,
        checks: {
          database: database_healthy?,
          redis: redis_healthy?,
          action_cable: action_cable_healthy?
        },
        metrics: {
          total_events: BankruptcyEvent.count,
          events_today: BankruptcyEvent.today.count,
          events_last_hour: BankruptcyEvent.where("detected_at >= ?", 1.hour.ago).count,
          last_event_at: BankruptcyEvent.order(detected_at: :desc).first&.detected_at&.iso8601,
          uptime_seconds: (Time.current - Rails.application.config.boot_time).to_i
        }
      }

      # Set overall status
      all_healthy = health[:checks].values.all?
      health[:status] = all_healthy ? "operational" : "degraded"

      status_code = all_healthy ? :ok : :service_unavailable
      render json: health, status: status_code
    end

    private

    def database_healthy?
      ActiveRecord::Base.connection.active?
    rescue StandardError
      false
    end

    def redis_healthy?
      Redis.new(url: ENV.fetch("REDIS_URL", "redis://localhost:6379/1")).ping == "PONG"
    rescue StandardError
      false
    end

    def action_cable_healthy?
      ActionCable.server.pubsub.present?
    rescue StandardError
      false
    end
  end
end
