import { Controller } from "@hotwired/stimulus"

// SystemHealthController
// Polls the Rust Doom Engine metrics endpoint and displays
// scanner health status, throughput, and uptime.
//
// HTML usage:
//   <div data-controller="system-health"
//        data-system-health-url-value="http://localhost:9090"
//        data-system-health-interval-value="3000">
//
//     <div data-system-health-target="statusIndicator" class="health-indicator"></div>
//     <span data-system-health-target="statusText">Checking...</span>
//
//     <div data-system-health-target="throughput">-- evt/s</div>
//     <div data-system-health-target="uptime">--:--:--</div>
//
//     <div data-system-health-target="scannerList">
//       <!-- Scanner status rows injected here -->
//     </div>
//
//     <div data-system-health-target="memoryUsage">-- MB</div>
//     <div data-system-health-target="queueDepth">--</div>
//   </div>

export default class extends Controller {
  static targets = [
    "statusIndicator",
    "statusText",
    "throughput",
    "uptime",
    "scannerList",
    "memoryUsage",
    "queueDepth",
    "lastCheck"
  ]

  static values = {
    url: { type: String, default: "http://localhost:9090" },
    interval: { type: Number, default: 3000 }
  }

  connect() {
    this.consecutiveFailures = 0
    this.maxFailuresBeforeRed = 3
    this.startTime = Date.now()

    // Initial check
    this._checkHealth()

    // Start polling
    this.pollTimer = setInterval(() => this._checkHealth(), this.intervalValue)

    // Update uptime display every second
    this.uptimeTimer = setInterval(() => this._updateUptimeDisplay(), 1000)

    console.log("[FREIGHT DOOM] System health monitor connected, polling %s every %dms",
      this.urlValue, this.intervalValue)
  }

  disconnect() {
    if (this.pollTimer) clearInterval(this.pollTimer)
    if (this.uptimeTimer) clearInterval(this.uptimeTimer)
  }

  // --- Actions ---

  forceCheck() {
    this._checkHealth()
  }

  // --- Private Methods ---

  async _checkHealth() {
    try {
      const healthResponse = await fetch(`${this.urlValue}/health`, {
        signal: AbortSignal.timeout(2000)
      })

      if (!healthResponse.ok) {
        throw new Error(`Health check returned ${healthResponse.status}`)
      }

      const healthData = await healthResponse.json()

      // Also fetch metrics
      let metricsData = null
      try {
        const metricsResponse = await fetch(`${this.urlValue}/metrics`, {
          signal: AbortSignal.timeout(2000)
        })
        if (metricsResponse.ok) {
          metricsData = await metricsResponse.json()
        }
      } catch (e) {
        // Metrics endpoint is optional
      }

      this.consecutiveFailures = 0
      this._updateDisplay(healthData, metricsData)
      this._setStatus("green", "ONLINE")

      // Update global state
      if (window.FreightDoom) {
        window.FreightDoom.engineOnline = true
      }

    } catch (error) {
      this.consecutiveFailures++

      if (this.consecutiveFailures >= this.maxFailuresBeforeRed) {
        this._setStatus("red", "OFFLINE")
        if (window.FreightDoom) {
          window.FreightDoom.engineOnline = false
        }
      } else {
        this._setStatus("yellow", "DEGRADED")
      }

      console.warn("[FREIGHT DOOM] Health check failed (%d/%d):",
        this.consecutiveFailures, this.maxFailuresBeforeRed, error.message)
    }

    // Update last check timestamp
    if (this.hasLastCheckTarget) {
      this.lastCheckTarget.textContent = new Date().toLocaleTimeString()
    }
  }

  _setStatus(color, text) {
    if (this.hasStatusIndicatorTarget) {
      // Remove all status classes
      this.statusIndicatorTarget.classList.remove(
        "health-indicator--green",
        "health-indicator--yellow",
        "health-indicator--red"
      )
      this.statusIndicatorTarget.classList.add(`health-indicator--${color}`)
    }

    if (this.hasStatusTextTarget) {
      this.statusTextTarget.textContent = text
      this.statusTextTarget.dataset.status = color
    }
  }

  _updateDisplay(healthData, metricsData) {
    // Throughput (events per second)
    if (this.hasThroughputTarget) {
      const eps = metricsData?.events_per_second ?? healthData?.events_per_second ?? 0
      this.throughputTarget.textContent = `${eps.toFixed(1)} evt/s`

      // Color code throughput
      this.throughputTarget.dataset.level =
        eps > 10 ? "high" :
        eps > 1 ? "medium" : "low"
    }

    // Engine uptime
    if (this.hasUptimeTarget && healthData?.uptime_seconds != null) {
      this.engineUptimeSeconds = healthData.uptime_seconds
      this._renderUptime(healthData.uptime_seconds)
    }

    // Scanner statuses
    if (this.hasScannerListTarget && healthData?.scanners) {
      this._renderScanners(healthData.scanners)
    }

    // Memory usage
    if (this.hasMemoryUsageTarget && metricsData?.memory_mb != null) {
      this.memoryUsageTarget.textContent = `${metricsData.memory_mb.toFixed(1)} MB`
    }

    // Queue depth
    if (this.hasQueueDepthTarget && metricsData?.queue_depth != null) {
      this.queueDepthTarget.textContent = metricsData.queue_depth.toLocaleString()
    }
  }

  _renderUptime(totalSeconds) {
    if (!this.hasUptimeTarget) return
    this.uptimeTarget.textContent = this._formatDuration(totalSeconds)
  }

  _updateUptimeDisplay() {
    // Increment the engine uptime each second (between polls)
    if (this.engineUptimeSeconds != null) {
      this.engineUptimeSeconds++
      this._renderUptime(this.engineUptimeSeconds)
    }
  }

  _formatDuration(totalSeconds) {
    const days = Math.floor(totalSeconds / 86400)
    const hours = Math.floor((totalSeconds % 86400) / 3600)
    const minutes = Math.floor((totalSeconds % 3600) / 60)
    const seconds = Math.floor(totalSeconds % 60)

    const pad = (n) => String(n).padStart(2, "0")

    if (days > 0) {
      return `${days}d ${pad(hours)}:${pad(minutes)}:${pad(seconds)}`
    }
    return `${pad(hours)}:${pad(minutes)}:${pad(seconds)}`
  }

  _renderScanners(scanners) {
    if (!this.hasScannerListTarget) return

    const html = scanners.map((scanner) => {
      const statusColor =
        scanner.status === "running" ? "green" :
        scanner.status === "idle" ? "yellow" :
        scanner.status === "error" ? "red" : "gray"

      const lastScan = scanner.last_scan_at
        ? new Date(scanner.last_scan_at).toLocaleTimeString()
        : "never"

      return `
        <div class="scanner-row">
          <span class="scanner-row__indicator health-indicator--${statusColor}"></span>
          <span class="scanner-row__name">${this._escapeHtml(scanner.name || scanner.source)}</span>
          <span class="scanner-row__status">${(scanner.status || "unknown").toUpperCase()}</span>
          <span class="scanner-row__last-scan">${lastScan}</span>
          <span class="scanner-row__count">${(scanner.events_found || 0).toLocaleString()} found</span>
        </div>
      `
    }).join("")

    this.scannerListTarget.innerHTML = html
  }

  _escapeHtml(text) {
    const div = document.createElement("div")
    div.textContent = text
    return div.innerHTML
  }
}
