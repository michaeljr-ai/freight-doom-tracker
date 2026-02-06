class CarriersController < ApplicationController
  include Pagy::Backend

  def index
    scope = Carrier.all
    scope = scope.search(params[:query]) if params[:query].present?
    scope = scope.by_type(params[:carrier_type]) if params[:carrier_type].present?
    scope = scope.where(status: params[:status]) if params[:status].present?
    scope = scope.order(name: :asc)

    @pagy, @carriers = pagy(scope, items: 25)
  end

  def show
    @carrier = Carrier.find(params[:id])
  end
end
