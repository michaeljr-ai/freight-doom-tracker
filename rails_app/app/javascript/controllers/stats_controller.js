import { Controller } from "@hotwired/stimulus"

// StatsController
// Polls the /stats endpoint and updates stat cards with animated number
// transitions and sparkline mini-charts.
//
// HTML usage:
//   <div data-controller="stats" data-stats-url-value="/stats" data-stats-interval-value="5000">
//     <div data-stats-target="card" data-stat-key="total_events">
//       <span data-stats-target="value">0</span>
//       <canvas data-stats-target="sparkline" width="120" height="30"></canvas>
//     </div>
//     <div data-stats-target="card" data-stat-key="events_today">
//       <span data-stats-target="value">0</span>
//       <canvas data-stats-target="sparkline" width="120" height="30"></canvas>
//     </div>
//   </div>

export default class extends Controller {
  static targets = ["card", "value", "sparkline"]
  static values = {
    url: { type: String, default: "/stats" },
    interval: { type: Number, default: 5000 }
  }

  connect() {
    // History buffers for sparklines (keyed by stat name)
    this.history = {}
    this.maxHistoryLength = 30 // 30 data points = ~2.5 minutes at 5s intervals
    this.currentValues = {}
    this.animatingValues = {}

    // Initial fetch
    this._fetchStats()

    // Start polling
    this.pollTimer = setInterval(() => this._fetchStats(), this.intervalValue)

    console.log("[FREIGHT DOOM] Stats controller connected, polling every %dms", this.intervalValue)
  }

  disconnect() {
    if (this.pollTimer) {
      clearInterval(this.pollTimer)
    }
  }

  // --- Private Methods ---

  async _fetchStats() {
    try {
      const response = await fetch(this.urlValue, {
        headers: {
          "Accept": "application/json",
          "X-Requested-With": "XMLHttpRequest"
        }
      })

      if (!response.ok) {
        console.warn("[FREIGHT DOOM] Stats fetch failed:", response.status)
        return
      }

      const data = await response.json()
      this._updateStats(data)
    } catch (error) {
      console.warn("[FREIGHT DOOM] Stats fetch error:", error.message)
    }
  }

  _updateStats(data) {
    // Iterate over each stat card and update values
    this.cardTargets.forEach((card) => {
      const key = card.dataset.statKey
      if (!key || !(key in data)) return

      const newValue = data[key]
      const oldValue = this.currentValues[key] || 0

      // Record history for sparkline
      if (!this.history[key]) {
        this.history[key] = []
      }
      this.history[key].push(newValue)
      if (this.history[key].length > this.maxHistoryLength) {
        this.history[key].shift()
      }

      // Animate the number transition
      if (newValue !== oldValue) {
        this._animateValue(card, key, oldValue, newValue)
      }

      // Update sparkline
      const sparklineCanvas = card.querySelector("[data-stats-target='sparkline']")
      if (sparklineCanvas) {
        this._drawSparkline(sparklineCanvas, this.history[key])
      }

      this.currentValues[key] = newValue
    })

    // Also update any standalone value targets not inside cards
    this.valueTargets.forEach((el) => {
      const key = el.dataset.statKey
      if (key && key in data) {
        const newValue = data[key]
        const oldValue = this.currentValues[key] || 0
        if (newValue !== oldValue) {
          this._animateValueElement(el, oldValue, newValue)
          this.currentValues[key] = newValue
        }
      }
    })
  }

  _animateValue(card, key, from, to) {
    // Find the value element within this card
    const valueEl = card.querySelector("[data-stats-target='value']")
    if (!valueEl) return

    this._animateValueElement(valueEl, from, to)

    // Add a subtle pulse to the card
    card.classList.add("stat-card--updated")
    setTimeout(() => card.classList.remove("stat-card--updated"), 800)
  }

  _animateValueElement(el, from, to) {
    // Cancel any existing animation for this element
    if (el._animFrame) {
      cancelAnimationFrame(el._animFrame)
    }

    const duration = 600 // ms
    const startTime = performance.now()
    const isFloat = !Number.isInteger(to)

    const step = (currentTime) => {
      const elapsed = currentTime - startTime
      const progress = Math.min(elapsed / duration, 1)

      // Ease-out exponential
      const eased = 1 - Math.pow(2, -10 * progress)
      const current = from + (to - from) * eased

      if (isFloat) {
        el.textContent = current.toFixed(1)
      } else {
        el.textContent = Math.round(current).toLocaleString()
      }

      if (progress < 1) {
        el._animFrame = requestAnimationFrame(step)
      } else {
        if (isFloat) {
          el.textContent = to.toFixed(1)
        } else {
          el.textContent = to.toLocaleString()
        }
        el._animFrame = null
      }
    }

    el._animFrame = requestAnimationFrame(step)
  }

  _drawSparkline(canvas, data) {
    if (!data || data.length < 2) return

    const ctx = canvas.getContext("2d")
    const width = canvas.width
    const height = canvas.height
    const padding = 2

    // Clear canvas
    ctx.clearRect(0, 0, width, height)

    // Calculate bounds
    const min = Math.min(...data)
    const max = Math.max(...data)
    const range = max - min || 1

    // Draw the sparkline
    const stepX = (width - padding * 2) / (data.length - 1)

    // Gradient fill under the line
    const gradient = ctx.createLinearGradient(0, 0, 0, height)
    gradient.addColorStop(0, "rgba(220, 38, 38, 0.3)")   // Red top
    gradient.addColorStop(1, "rgba(220, 38, 38, 0.02)")  // Transparent bottom

    // Fill area
    ctx.beginPath()
    ctx.moveTo(padding, height)
    for (let i = 0; i < data.length; i++) {
      const x = padding + i * stepX
      const y = height - padding - ((data[i] - min) / range) * (height - padding * 2)
      ctx.lineTo(x, y)
    }
    ctx.lineTo(padding + (data.length - 1) * stepX, height)
    ctx.closePath()
    ctx.fillStyle = gradient
    ctx.fill()

    // Draw line
    ctx.beginPath()
    for (let i = 0; i < data.length; i++) {
      const x = padding + i * stepX
      const y = height - padding - ((data[i] - min) / range) * (height - padding * 2)
      if (i === 0) {
        ctx.moveTo(x, y)
      } else {
        ctx.lineTo(x, y)
      }
    }
    ctx.strokeStyle = "#dc2626"
    ctx.lineWidth = 1.5
    ctx.stroke()

    // Draw latest value dot
    const lastX = padding + (data.length - 1) * stepX
    const lastY = height - padding - ((data[data.length - 1] - min) / range) * (height - padding * 2)
    ctx.beginPath()
    ctx.arc(lastX, lastY, 2.5, 0, Math.PI * 2)
    ctx.fillStyle = "#ef4444"
    ctx.fill()

    // Glow on latest dot
    ctx.beginPath()
    ctx.arc(lastX, lastY, 5, 0, Math.PI * 2)
    ctx.fillStyle = "rgba(239, 68, 68, 0.3)"
    ctx.fill()
  }
}
