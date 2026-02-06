module Api
  class EventsController < ActionController::API
    # POST /api/events
    # Receives bankruptcy event data from the Rust engine
    def create
      @event = BankruptcyEvent.new(event_params)
      @event.detected_at ||= Time.current
      @event.source ||= "rust_engine"

      if @event.save
        # The after_create_commit callback on the model handles Turbo Stream broadcast.
        # Also broadcast via the channel for non-Turbo clients.
        BankruptcyEventsChannel.broadcast_event(@event)
        BankruptcyEventsChannel.broadcast_stats_update

        Rails.logger.info "[DOOM TRACKER] New bankruptcy event: #{@event.company_name} (#{@event.source})"

        render json: {
          status: "created",
          event: event_json(@event)
        }, status: :created
      else
        render json: {
          status: "error",
          errors: @event.errors.full_messages
        }, status: :unprocessable_entity
      end
    end

    # GET /api/events
    # Returns recent events as JSON
    def index
      limit = (params[:limit] || 50).to_i.clamp(1, 500)
      events = BankruptcyEvent.recent(limit)

      # Apply filters if provided
      events = events.by_source(params[:source]) if params[:source].present?
      events = events.by_chapter(params[:chapter].to_i) if params[:chapter].present?
      events = events.by_status(params[:status]) if params[:status].present?

      render json: {
        count: events.size,
        events: events.map { |e| event_json(e) }
      }
    end

    private

    def event_params
      params.require(:event).permit(
        :company_name, :dot_number, :mc_number,
        :filing_date, :court, :chapter,
        :source, :confidence_score, :status,
        :detected_at,
        raw_data: {}
      )
    end

    def event_json(event)
      {
        id: event.id,
        company_name: event.company_name,
        dot_number: event.dot_number,
        mc_number: event.mc_number,
        filing_date: event.filing_date,
        court: event.court,
        chapter: event.chapter,
        source: event.source,
        confidence_score: event.confidence_score,
        confidence_level: event.confidence_level,
        status: event.status,
        detected_at: event.detected_at&.iso8601,
        time_ago: event.time_ago_detected,
        created_at: event.created_at.iso8601
      }
    end
  end
end
