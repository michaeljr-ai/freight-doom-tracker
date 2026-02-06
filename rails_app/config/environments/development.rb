require "active_support/core_ext/integer/time"
Rails.application.configure do
  config.enable_reloading = true
  config.eager_load = false
  config.consider_all_requests_local = true
  config.action_controller.perform_caching = false
  config.active_storage.service = :local rescue nil
  config.action_mailer.raise_delivery_errors = false rescue nil
  config.active_support.deprecation = :log
  config.active_record.migration_error = :page_load
  config.action_cable.disable_request_forgery_protection = true
end
