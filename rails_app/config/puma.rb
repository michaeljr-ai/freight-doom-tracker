# Puma configuration for the Freight Doom Tracker
# Optimized for Action Cable WebSocket connections

# Thread pool sizing
max_threads_count = ENV.fetch("RAILS_MAX_THREADS") { 5 }
min_threads_count = ENV.fetch("RAILS_MIN_THREADS") { max_threads_count }
threads min_threads_count, max_threads_count

# Worker count (0 in development for easier debugging)
worker_count = ENV.fetch("WEB_CONCURRENCY") { 0 }
workers worker_count if worker_count.to_i > 0

# Port binding
port ENV.fetch("PORT") { 3000 }

# Environment
environment ENV.fetch("RAILS_ENV") { "development" }

# PID file
pidfile ENV.fetch("PIDFILE") { "tmp/pids/server.pid" }

# Allow puma to be restarted by `bin/rails restart`
plugin :tmp_restart

# Preload app for faster worker boot (production)
if ENV.fetch("RAILS_ENV", "development") == "production"
  preload_app!

  on_worker_boot do
    ActiveRecord::Base.establish_connection
  end
end

# Action Cable requires this for WebSocket support
# Bind to all interfaces
bind "tcp://0.0.0.0:#{ENV.fetch('PORT') { 3000 }}"
