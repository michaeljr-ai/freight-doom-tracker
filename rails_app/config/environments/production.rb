require "active_support/core_ext/integer/time"
Rails.application.configure do
  config.enable_reloading = false
  config.eager_load = true
  config.consider_all_requests_local = false
  config.force_ssl = false
  config.assume_ssl = false
  config.log_level = :info
  config.log_tags = [:request_id]
  config.logger = ActiveSupport::Logger.new(STDOUT)
    .tap  { |logger| logger.formatter = ::Logger::Formatter.new }
    .then { |logger| ActiveSupport::TaggedLogging.new(logger) }

  # Action Cable — allow any origin for Railway deployment
  config.action_cable.disable_request_forgery_protection = true
  config.action_cable.allowed_request_origins = [/https?:\/\/.*/]

  # Allow Railway hosts
  config.hosts.clear

  # Serve static files (Railway has no Nginx in front)
  config.public_file_server.enabled = true

  config.active_support.report_deprecations = false
  config.active_record.dump_schema_after_migration = false
end
