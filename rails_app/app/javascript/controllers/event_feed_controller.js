import { Controller } from "@hotwired/stimulus"

// EventFeedController
// Manages the live scrolling feed of bankruptcy events.
// Auto-scrolls to newest events, highlights new arrivals with animation,
// and provides filter controls for source/severity.
//
// HTML usage:
//   <div data-controller="event-feed"
//        data-event-feed-auto-scroll-value="true">
//     <div data-event-feed-target="filters">
//       <button data-action="event-feed#filterBySource" data-source="pacer">PACER</button>
//       <button data-action="event-feed#filterBySource" data-source="edgar">EDGAR</button>
//       <button data-action="event-feed#filterBySource" data-source="fmcsa">FMCSA</button>
//       <button data-action="event-feed#filterBySource" data-source="courtlistener">CourtListener</button>
//       <button data-action="event-feed#clearFilters">ALL</button>
//     </div>
//     <div data-event-feed-target="feed" class="event-feed-container">
//       <!-- Events are appended here via Turbo Streams or manual insertion -->
//     </div>
//     <div data-event-feed-target="emptyState" class="hidden">
//       No events match your filters.
//     </div>
//   </div>

export default class extends Controller {
  static targets = ["feed", "filters", "emptyState", "eventCount"]
  static values = {
    autoScroll: { type: Boolean, default: true },
    activeFilter: { type: String, default: "all" },
    maxEvents: { type: Number, default: 500 }
  }

  connect() {
    this.userScrolled = false

    // Listen for scroll events to detect manual scrolling
    if (this.hasFeedTarget) {
      this.feedTarget.addEventListener("scroll", this._onScroll.bind(this))
    }

    // Listen for new events from Action Cable / Turbo Streams
    window.addEventListener("freight-doom:new-event", this._onNewEvent.bind(this))

    // Observe DOM mutations for Turbo Stream insertions
    this.observer = new MutationObserver((mutations) => {
      for (const mutation of mutations) {
        for (const node of mutation.addedNodes) {
          if (node.nodeType === Node.ELEMENT_NODE && node.classList?.contains("event-card")) {
            this._highlightNewEvent(node)
          }
        }
      }
    })

    if (this.hasFeedTarget) {
      this.observer.observe(this.feedTarget, { childList: true })
    }

    console.log("[FREIGHT DOOM] Event feed controller connected")
  }

  disconnect() {
    if (this.observer) {
      this.observer.disconnect()
    }
    window.removeEventListener("freight-doom:new-event", this._onNewEvent.bind(this))
  }

  // --- Actions ---

  filterBySource(event) {
    const source = event.currentTarget.dataset.source || event.params?.source
    if (!source) return

    this.activeFilterValue = source
    this._applyFilter(source)
    this._updateFilterButtons(source)
  }

  clearFilters() {
    this.activeFilterValue = "all"
    this._applyFilter("all")
    this._updateFilterButtons("all")
  }

  toggleAutoScroll() {
    this.autoScrollValue = !this.autoScrollValue
    this.userScrolled = !this.autoScrollValue
  }

  // --- Manual event insertion (for use without Turbo Streams) ---

  addEvent(eventData) {
    if (!this.hasFeedTarget) return

    const card = this._buildEventCard(eventData)
    this.feedTarget.prepend(card)

    this._highlightNewEvent(card)
    this._pruneOldEvents()
    this._updateEventCount()

    if (this.autoScrollValue && !this.userScrolled) {
      this._scrollToTop()
    }
  }

  // --- Private Methods ---

  _onNewEvent(event) {
    const data = event.detail
    if (data) {
      this.addEvent(data)
    }
  }

  _buildEventCard(data) {
    const card = document.createElement("div")
    card.className = `event-card event-card--${(data.source || "unknown").toLowerCase()}`
    card.dataset.source = (data.source || "unknown").toLowerCase()
    card.dataset.severity = data.severity || "info"
    card.dataset.eventId = data.id || Date.now()

    const confidencePercent = Math.round((data.confidence || 0) * 100)
    const timestamp = data.detected_at
      ? new Date(data.detected_at).toLocaleTimeString()
      : new Date().toLocaleTimeString()

    card.innerHTML = `
      <div class="event-card__header">
        <span class="event-card__source badge badge--${(data.source || "unknown").toLowerCase()}">
          ${(data.source || "UNKNOWN").toUpperCase()}
        </span>
        <span class="event-card__time">${timestamp}</span>
      </div>
      <div class="event-card__body">
        <h4 class="event-card__company">${this._escapeHtml(data.company_name || "Unknown Entity")}</h4>
        <p class="event-card__detail">${this._escapeHtml(data.filing_type || data.event_type || "Bankruptcy Filing Detected")}</p>
      </div>
      <div class="event-card__footer">
        <div class="confidence-bar">
          <div class="confidence-bar__fill" style="width: ${confidencePercent}%"></div>
          <span class="confidence-bar__label">${confidencePercent}% confidence</span>
        </div>
        ${data.case_number ? `<span class="event-card__case">Case: ${this._escapeHtml(data.case_number)}</span>` : ""}
      </div>
    `

    // Apply filter visibility
    if (this.activeFilterValue !== "all" &&
        card.dataset.source !== this.activeFilterValue) {
      card.style.display = "none"
    }

    return card
  }

  _highlightNewEvent(node) {
    // Add slide-in animation class
    node.classList.add("event-card--entering")

    // Flash effect
    requestAnimationFrame(() => {
      node.classList.add("event-card--flash")
      setTimeout(() => {
        node.classList.remove("event-card--flash")
        node.classList.remove("event-card--entering")
      }, 1500)
    })
  }

  _applyFilter(source) {
    if (!this.hasFeedTarget) return

    const cards = this.feedTarget.querySelectorAll(".event-card")
    let visibleCount = 0

    cards.forEach((card) => {
      if (source === "all" || card.dataset.source === source) {
        card.style.display = ""
        visibleCount++
      } else {
        card.style.display = "none"
      }
    })

    // Show/hide empty state
    if (this.hasEmptyStateTarget) {
      this.emptyStateTarget.classList.toggle("hidden", visibleCount > 0)
    }

    this._updateEventCount()
  }

  _updateFilterButtons(activeSource) {
    if (!this.hasFiltersTarget) return

    const buttons = this.filtersTarget.querySelectorAll("[data-source], [data-action*='clearFilters']")
    buttons.forEach((btn) => {
      const isActive = activeSource === "all"
        ? btn.dataset.action?.includes("clearFilters")
        : btn.dataset.source === activeSource

      btn.classList.toggle("filter-btn--active", isActive)
    })
  }

  _pruneOldEvents() {
    if (!this.hasFeedTarget) return

    const cards = this.feedTarget.querySelectorAll(".event-card")
    if (cards.length > this.maxEventsValue) {
      // Remove oldest events (from the bottom)
      const excess = cards.length - this.maxEventsValue
      for (let i = cards.length - 1; i >= cards.length - excess; i--) {
        cards[i].remove()
      }
    }
  }

  _updateEventCount() {
    if (!this.hasEventCountTarget || !this.hasFeedTarget) return

    const visible = this.feedTarget.querySelectorAll('.event-card:not([style*="display: none"])')
    this.eventCountTarget.textContent = visible.length.toLocaleString()
  }

  _scrollToTop() {
    if (!this.hasFeedTarget) return
    this.feedTarget.scrollTo({
      top: 0,
      behavior: "smooth"
    })
  }

  _onScroll() {
    if (!this.hasFeedTarget) return

    // If user scrolls away from top, disable auto-scroll
    const scrollTop = this.feedTarget.scrollTop
    this.userScrolled = scrollTop > 100

    // Re-enable auto-scroll when user scrolls back to top
    if (scrollTop < 10) {
      this.userScrolled = false
    }
  }

  _escapeHtml(text) {
    const div = document.createElement("div")
    div.textContent = text
    return div.innerHTML
  }
}
