class BankruptcyEvent < ApplicationRecord
  # ---------------------------------------------------------------------------
  # Enums
  # ---------------------------------------------------------------------------
  enum :source, {
    pacer:       "pacer",
    fmcsa:       "fmcsa",
    court_feed:  "court_feed",
    news:        "news",
    sec_filing:  "sec_filing",
    manual:      "manual",
    rust_engine: "rust_engine"
  }, prefix: true

  enum :status, {
    new_event:   "new",
    confirmed:   "confirmed",
    investigating: "investigating",
    resolved:    "resolved",
    false_alarm: "false_alarm"
  }, prefix: true

  # ---------------------------------------------------------------------------
  # Validations
  # ---------------------------------------------------------------------------
  validates :company_name, presence: true
  validates :source, presence: true
  validates :confidence_score, numericality: {
    greater_than_or_equal_to: 0.0,
    less_than_or_equal_to: 1.0
  }, allow_nil: true
  validates :chapter, inclusion: { in: [7, 11, 13, 15] }, allow_nil: true

  # ---------------------------------------------------------------------------
  # Scopes
  # ---------------------------------------------------------------------------
  scope :recent, ->(limit = 50) { order(detected_at: :desc).limit(limit) }
  scope :today, -> { where("detected_at >= ?", Time.current.beginning_of_day) }
  scope :this_week, -> { where("detected_at >= ?", 1.week.ago) }
  scope :this_month, -> { where("detected_at >= ?", 1.month.ago) }
  scope :by_source, ->(source) { where(source: source) }
  scope :by_chapter, ->(chapter) { where(chapter: chapter) }
  scope :by_status, ->(status) { where(status: status) }
  scope :high_confidence, -> { where("confidence_score >= ?", 0.8) }
  scope :low_confidence, -> { where("confidence_score < ?", 0.5) }
  scope :unresolved, -> { where.not(status: ["resolved", "false_alarm"]) }

  # ---------------------------------------------------------------------------
  # Callbacks - Turbo Stream broadcasts
  # ---------------------------------------------------------------------------
  after_create_commit :broadcast_new_event
  after_update_commit :broadcast_updated_event

  # ---------------------------------------------------------------------------
  # Class Methods
  # ---------------------------------------------------------------------------

  # Search by company name, DOT number, or MC number
  def self.search(query)
    return all if query.blank?

    where(
      "company_name LIKE :q OR dot_number LIKE :q OR mc_number LIKE :q",
      q: "%#{query}%"
    )
  end

  # Filter by multiple criteria
  def self.filter_by(params)
    scope = all
    scope = scope.search(params[:query]) if params[:query].present?
    scope = scope.by_source(params[:source]) if params[:source].present?
    scope = scope.by_chapter(params[:chapter].to_i) if params[:chapter].present?
    scope = scope.by_status(params[:status]) if params[:status].present?
    scope = scope.where("confidence_score >= ?", params[:min_confidence].to_f) if params[:min_confidence].present?
    scope
  end

  # Stats for the dashboard
  def self.dashboard_stats
    {
      total: count,
      today: today.count,
      this_week: this_week.count,
      this_month: this_month.count,
      by_chapter: group(:chapter).count,
      by_source: group(:source).count,
      by_status: group(:status).count,
      avg_confidence: average(:confidence_score)&.round(2) || 0,
      high_confidence_count: high_confidence.count
    }
  end

  # Chart data: events over time (last 30 days)
  def self.events_over_time(days = 30)
    where("detected_at >= ?", days.days.ago)
      .group_by_day(:detected_at)
      .count
  end

  # Chart data: events by source
  def self.events_by_source
    group(:source).count
  end

  # Chart data: events by chapter
  def self.events_by_chapter
    where.not(chapter: nil).group(:chapter).count
  end

  # ---------------------------------------------------------------------------
  # Instance Methods
  # ---------------------------------------------------------------------------
  def chapter_label
    return "Unknown" unless chapter
    "Chapter #{chapter}"
  end

  def confidence_percentage
    ((confidence_score || 0) * 100).round(0)
  end

  def confidence_level
    score = confidence_score || 0
    case score
    when 0.8..1.0 then "high"
    when 0.5..0.8 then "medium"
    else "low"
    end
  end

  def time_ago_detected
    return "Unknown" unless detected_at
    seconds = (Time.current - detected_at).to_i
    case seconds
    when 0..59 then "#{seconds}s ago"
    when 60..3599 then "#{seconds / 60}m ago"
    when 3600..86399 then "#{seconds / 3600}h ago"
    else "#{seconds / 86400}d ago"
    end
  end

  def source_color
    case source
    when "pacer" then "blue"
    when "fmcsa" then "green"
    when "court_feed" then "purple"
    when "news" then "yellow"
    when "sec_filing" then "orange"
    when "rust_engine" then "red"
    when "manual" then "gray"
    else "gray"
    end
  end

  private

  def broadcast_new_event
    # Broadcast to the Turbo Stream channel for live dashboard updates
    broadcast_prepend_to(
      "bankruptcy_events",
      target: "bankruptcy_events_feed",
      partial: "bankruptcy_events/bankruptcy_event",
      locals: { bankruptcy_event: self }
    )

    # Also broadcast an event count update
    broadcast_replace_to(
      "bankruptcy_events",
      target: "event_counter",
      html: "<span id='event_counter' class='text-red-400 font-mono text-4xl animate-pulse'>#{BankruptcyEvent.count}</span>"
    )
  end

  def broadcast_updated_event
    broadcast_replace_to(
      "bankruptcy_events",
      target: self,
      partial: "bankruptcy_events/bankruptcy_event",
      locals: { bankruptcy_event: self }
    )
  end
end
