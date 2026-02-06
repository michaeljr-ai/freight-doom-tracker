Rails.application.routes.draw do
  # Dashboard - main landing page
  root "dashboard#index"
  get "/stats", to: "dashboard#stats"

  # Bankruptcy events
  resources :bankruptcy_events, only: [:index, :show]

  # Carriers
  resources :carriers, only: [:index, :show]

  # Action Cable WebSocket endpoint
  mount ActionCable.server => "/cable"

  # API namespace for Rust engine integration
  namespace :api do
    resources :events, only: [:index, :create]
    resources :health, only: [:index]
  end

  # Health check at root level too
  get "/health", to: "api/health#index"
end
