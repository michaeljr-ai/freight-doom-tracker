require_relative "boot"
require "rails/all"
Bundler.require(*Rails.groups)

module FreightDoomTracker
  class Application < Rails::Application
    config.load_defaults 7.1
    config.time_zone = "UTC"
    config.api_only = false
  end
end
