class DashboardController < ApplicationController
  include Pagy::Backend

  def index
    # Recent events for the live feed
    @recent_events = BankruptcyEvent.recent(25)

    # Dashboard statistics
    @stats = BankruptcyEvent.dashboard_stats

    # Chart data
    @events_over_time = BankruptcyEvent.events_over_time(30)
    @events_by_source = BankruptcyEvent.events_by_source
    @events_by_chapter = BankruptcyEvent.events_by_chapter

    # Today's high confidence alerts
    @critical_alerts = BankruptcyEvent.today.high_confidence.recent(10)

    # System health
    @system_health = {
      redis_connected: redis_connected?,
      last_event_at: BankruptcyEvent.order(detected_at: :desc).first&.detected_at,
      events_last_hour: BankruptcyEvent.where("detected_at >= ?", 1.hour.ago).count,
      db_size: BankruptcyEvent.count
    }
  end

  def stats
    render json: {
      stats: BankruptcyEvent.dashboard_stats,
      events_over_time: BankruptcyEvent.events_over_time(30),
      events_by_source: BankruptcyEvent.events_by_source,
      events_by_chapter: BankruptcyEvent.events_by_chapter,
      system_health: {
        redis_connected: redis_connected?,
        total_events: BankruptcyEvent.count,
        events_today: BankruptcyEvent.today.count,
        last_event_at: BankruptcyEvent.order(detected_at: :desc).first&.detected_at
      }
    }
  end

  private

  def redis_connected?
    Redis.new(url: ENV.fetch("REDIS_URL", "redis://localhost:6379/1")).ping == "PONG"
  rescue StandardError
    false
  end
end
