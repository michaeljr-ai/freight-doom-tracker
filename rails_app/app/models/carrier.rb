class Carrier < ApplicationRecord
  # ---------------------------------------------------------------------------
  # Enums
  # ---------------------------------------------------------------------------
  enum :carrier_type, {
    carrier:           "carrier",
    broker:            "broker",
    three_pl:          "3pl",
    freight_forwarder: "freight_forwarder"
  }, prefix: true

  enum :status, {
    active:    "active",
    inactive:  "inactive",
    suspended: "suspended",
    revoked:   "revoked",
    bankrupt:  "bankrupt"
  }, prefix: true

  # ---------------------------------------------------------------------------
  # Validations
  # ---------------------------------------------------------------------------
  validates :name, presence: true
  validates :dot_number, uniqueness: true, allow_nil: true

  # ---------------------------------------------------------------------------
  # Scopes
  # ---------------------------------------------------------------------------
  scope :active_carriers, -> { where(status: "active") }
  scope :bankrupt_carriers, -> { where(status: "bankrupt") }
  scope :by_type, ->(type) { where(carrier_type: type) }
  scope :recently_checked, -> { where("last_checked_at >= ?", 24.hours.ago) }
  scope :needs_check, -> { where("last_checked_at < ? OR last_checked_at IS NULL", 24.hours.ago) }
  scope :search, ->(query) {
    return all if query.blank?
    where("name LIKE :q OR dot_number LIKE :q OR mc_number LIKE :q", q: "%#{query}%")
  }

  # ---------------------------------------------------------------------------
  # Instance Methods
  # ---------------------------------------------------------------------------
  def display_type
    case carrier_type
    when "carrier" then "Motor Carrier"
    when "broker" then "Freight Broker"
    when "3pl" then "3PL Provider"
    when "freight_forwarder" then "Freight Forwarder"
    else carrier_type&.titleize || "Unknown"
    end
  end

  def status_color
    case status
    when "active" then "green"
    when "inactive" then "gray"
    when "suspended" then "yellow"
    when "revoked" then "orange"
    when "bankrupt" then "red"
    else "gray"
    end
  end

  def mark_bankrupt!
    update!(status: "bankrupt")
  end
end
