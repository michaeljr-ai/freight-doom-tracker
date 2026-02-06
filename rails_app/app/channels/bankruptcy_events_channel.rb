class BankruptcyEventsChannel < ApplicationCable::Channel
  def subscribed
    stream_from "bankruptcy_events"
    logger.info "[DOOM TRACKER] Client subscribed to bankruptcy_events channel"
  end

  def unsubscribed
    logger.info "[DOOM TRACKER] Client unsubscribed from bankruptcy_events channel"
  end

  # Broadcast a new event to all connected clients as a Turbo Stream
  def self.broadcast_event(event)
    ActionCable.server.broadcast(
      "bankruptcy_events",
      {
        type: "new_event",
        event: {
          id: event.id,
          company_name: event.company_name,
          dot_number: event.dot_number,
          mc_number: event.mc_number,
          chapter: event.chapter,
          source: event.source,
          confidence_score: event.confidence_score,
          detected_at: event.detected_at.iso8601,
          status: event.status
        }
      }
    )
  end

  # Broadcast stats update to all clients
  def self.broadcast_stats_update
    stats = BankruptcyEvent.dashboard_stats
    ActionCable.server.broadcast(
      "bankruptcy_events",
      {
        type: "stats_update",
        stats: stats
      }
    )
  end
end
