import { Controller } from "@hotwired/stimulus"
import { createConsumer } from "@rails/actioncable"

// LiveCounterController
// Connects to Action Cable and animates bankruptcy event counters in real-time.
// Pulses cards red and plays a doom sound on new detections.
//
// HTML usage:
//   <div data-controller="live-counter"
//        data-live-counter-channel-value="BankruptcyEventsChannel">
//     <span data-live-counter-target="count">0</span>
//     <span data-live-counter-target="lastUpdated">just now</span>
//   </div>

export default class extends Controller {
  static targets = ["count", "lastUpdated", "card"]
  static values = {
    channel: { type: String, default: "BankruptcyEventsChannel" },
    current: { type: Number, default: 0 }
  }

  connect() {
    this.consumer = createConsumer()
    this.lastEventTime = Date.now()
    this.animating = false

    // Subscribe to Action Cable channel
    this.subscription = this.consumer.subscriptions.create(
      { channel: this.channelValue },
      {
        connected: () => {
          console.log("[FREIGHT DOOM] Live counter connected to Action Cable")
          this._setConnectionStatus(true)
        },

        disconnected: () => {
          console.log("[FREIGHT DOOM] Live counter disconnected")
          this._setConnectionStatus(false)
        },

        received: (data) => {
          this._handleEvent(data)
        }
      }
    )

    // Start the "last updated" ticker
    this.tickerInterval = setInterval(() => this._updateLastUpdated(), 1000)

    // Listen for cross-controller events
    window.addEventListener("freight-doom:new-event", this._onExternalEvent.bind(this))
  }

  disconnect() {
    if (this.subscription) {
      this.subscription.unsubscribe()
    }
    if (this.consumer) {
      this.consumer.disconnect()
    }
    if (this.tickerInterval) {
      clearInterval(this.tickerInterval)
    }
    if (this.animationFrame) {
      cancelAnimationFrame(this.animationFrame)
    }
    window.removeEventListener("freight-doom:new-event", this._onExternalEvent.bind(this))
  }

  // --- Private Methods ---

  _handleEvent(data) {
    const newCount = data.total_count || (this.currentValue + 1)
    this.lastEventTime = Date.now()

    // Animate the count transition
    this._animateCountTo(newCount)

    // Pulse the card red
    this._pulseCard()

    // Play the doom sound
    if (window.FreightDoom) {
      window.FreightDoom.playDoomSound()
      window.FreightDoom.eventsReceived++
      window.FreightDoom.lastEventAt = new Date()
      window.FreightDoom.dispatch("new-event", data)
    }

    // Update the last updated display immediately
    this._updateLastUpdated()
  }

  _animateCountTo(target) {
    if (this.animating) {
      // If already animating, just jump to the target
      this.currentValue = target
      this._renderCount(target)
      return
    }

    this.animating = true
    const start = this.currentValue
    const diff = target - start
    const duration = Math.min(Math.abs(diff) * 50, 1000) // Max 1 second animation
    const startTime = performance.now()

    const step = (currentTime) => {
      const elapsed = currentTime - startTime
      const progress = Math.min(elapsed / duration, 1)

      // Ease-out cubic for smooth deceleration
      const eased = 1 - Math.pow(1 - progress, 3)
      const current = Math.round(start + diff * eased)

      this._renderCount(current)

      if (progress < 1) {
        this.animationFrame = requestAnimationFrame(step)
      } else {
        this.currentValue = target
        this._renderCount(target)
        this.animating = false
      }
    }

    this.animationFrame = requestAnimationFrame(step)
  }

  _renderCount(value) {
    if (this.hasCountTarget) {
      // Format with commas for readability
      this.countTarget.textContent = value.toLocaleString()

      // Add a brief scale animation on change
      this.countTarget.classList.add("counter-tick")
      setTimeout(() => {
        this.countTarget.classList.remove("counter-tick")
      }, 150)
    }
  }

  _pulseCard() {
    if (this.hasCardTarget) {
      this.cardTarget.classList.add("card-pulse-alert")
      setTimeout(() => {
        this.cardTarget.classList.remove("card-pulse-alert")
      }, 2000)
    }
  }

  _updateLastUpdated() {
    if (!this.hasLastUpdatedTarget) return

    const now = Date.now()
    const diffMs = now - this.lastEventTime
    const diffSec = Math.floor(diffMs / 1000)

    let text
    if (diffSec < 5) {
      text = "just now"
    } else if (diffSec < 60) {
      text = `${diffSec}s ago`
    } else if (diffSec < 3600) {
      const min = Math.floor(diffSec / 60)
      text = `${min}m ago`
    } else {
      const hrs = Math.floor(diffSec / 3600)
      text = `${hrs}h ago`
    }

    this.lastUpdatedTarget.textContent = text
  }

  _setConnectionStatus(connected) {
    const indicator = this.element.querySelector("[data-connection-status]")
    if (indicator) {
      indicator.dataset.status = connected ? "connected" : "disconnected"
      indicator.textContent = connected ? "LIVE" : "OFFLINE"
    }
  }

  _onExternalEvent(event) {
    // Handle events dispatched from other controllers
    const data = event.detail
    if (data && data.total_count) {
      this._animateCountTo(data.total_count)
    }
  }
}
