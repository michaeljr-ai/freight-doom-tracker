class AlertSubscription < ApplicationRecord
  # ---------------------------------------------------------------------------
  # Validations
  # ---------------------------------------------------------------------------
  validates :email, presence: true,
                    format: { with: URI::MailTo::EMAIL_REGEXP, message: "must be a valid email" }

  # ---------------------------------------------------------------------------
  # Scopes
  # ---------------------------------------------------------------------------
  scope :active_subscriptions, -> { where(active: true) }
  scope :by_carrier_type, ->(type) { where(carrier_type_filter: type) }

  # ---------------------------------------------------------------------------
  # Instance Methods
  # ---------------------------------------------------------------------------
  def matches_event?(event)
    return false unless active?

    # Check carrier type filter
    if carrier_type_filter.present?
      return false unless event.company_name.downcase.include?(carrier_type_filter.downcase)
    end

    # Check keyword filter
    if keyword_filter.present?
      keywords = keyword_filter.split(",").map(&:strip).map(&:downcase)
      event_text = "#{event.company_name} #{event.court} #{event.source}".downcase
      return false unless keywords.any? { |kw| event_text.include?(kw) }
    end

    true
  end

  def deactivate!
    update!(active: false)
  end

  def activate!
    update!(active: true)
  end
end
