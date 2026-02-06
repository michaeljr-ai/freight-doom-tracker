class BankruptcyEventsController < ApplicationController
  include Pagy::Backend

  def index
    scope = BankruptcyEvent.filter_by(filter_params)

    # Sorting
    sort_column = %w[company_name detected_at chapter source confidence_score].include?(params[:sort]) ? params[:sort] : "detected_at"
    sort_direction = %w[asc desc].include?(params[:direction]) ? params[:direction] : "desc"
    scope = scope.order(sort_column => sort_direction)

    @pagy, @bankruptcy_events = pagy(scope, items: 25)

    # Stats for the filter sidebar
    @source_counts = BankruptcyEvent.group(:source).count
    @chapter_counts = BankruptcyEvent.where.not(chapter: nil).group(:chapter).count
    @status_counts = BankruptcyEvent.group(:status).count

    respond_to do |format|
      format.html
      format.turbo_stream
      format.json { render json: @bankruptcy_events }
    end
  end

  def show
    @bankruptcy_event = BankruptcyEvent.find(params[:id])

    # Find related events (same company or DOT number)
    @related_events = BankruptcyEvent.where.not(id: @bankruptcy_event.id)
      .where(
        "company_name = :name OR dot_number = :dot",
        name: @bankruptcy_event.company_name,
        dot: @bankruptcy_event.dot_number
      )
      .recent(10)
  end

  private

  def filter_params
    params.permit(:query, :source, :chapter, :status, :min_confidence)
  end
end
