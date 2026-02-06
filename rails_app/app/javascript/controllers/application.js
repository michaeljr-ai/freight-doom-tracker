import { Application } from "@hotwired/stimulus"

const application = Application.start()

// Configure Stimulus development experience
application.debug = false
window.Stimulus = application

// Global doom state for cross-controller communication
window.FreightDoom = {
  eventsReceived: 0,
  lastEventAt: null,
  engineOnline: false,
  soundEnabled: true,

  // Create a subtle doom rumble sound using Web Audio API
  playDoomSound() {
    if (!this.soundEnabled) return

    try {
      const audioCtx = new (window.AudioContext || window.webkitAudioContext)()

      // Low rumble oscillator
      const oscillator = audioCtx.createOscillator()
      const gainNode = audioCtx.createGain()

      oscillator.connect(gainNode)
      gainNode.connect(audioCtx.destination)

      oscillator.type = "sine"
      oscillator.frequency.setValueAtTime(65, audioCtx.currentTime) // Deep bass
      oscillator.frequency.exponentialRampToValueAtTime(40, audioCtx.currentTime + 0.3)

      gainNode.gain.setValueAtTime(0.15, audioCtx.currentTime)
      gainNode.gain.exponentialRampToValueAtTime(0.001, audioCtx.currentTime + 0.5)

      oscillator.start(audioCtx.currentTime)
      oscillator.stop(audioCtx.currentTime + 0.5)

      // Secondary higher tone for alert feel
      const osc2 = audioCtx.createOscillator()
      const gain2 = audioCtx.createGain()

      osc2.connect(gain2)
      gain2.connect(audioCtx.destination)

      osc2.type = "triangle"
      osc2.frequency.setValueAtTime(220, audioCtx.currentTime)
      osc2.frequency.exponentialRampToValueAtTime(110, audioCtx.currentTime + 0.2)

      gain2.gain.setValueAtTime(0.08, audioCtx.currentTime)
      gain2.gain.exponentialRampToValueAtTime(0.001, audioCtx.currentTime + 0.3)

      osc2.start(audioCtx.currentTime)
      osc2.stop(audioCtx.currentTime + 0.3)
    } catch (e) {
      // Audio not available, silently continue
    }
  },

  // Dispatch a custom event for cross-controller communication
  dispatch(eventName, detail = {}) {
    window.dispatchEvent(new CustomEvent(`freight-doom:${eventName}`, { detail }))
  }
}

export { application }
