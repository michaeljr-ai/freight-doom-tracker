module ApplicationCable
  class Connection < ActionCable::Connection::Base
    # For a public dashboard, we don't require authentication.
    # In production, you'd identify the connection:
    #
    #   identified_by :current_user
    #
    #   def connect
    #     self.current_user = find_verified_user
    #   end
    #
    #   def find_verified_user
    #     User.find_by(id: cookies.encrypted[:user_id]) || reject_unauthorized_connection
    #   end

    def connect
      logger.add_tags "ActionCable", "FreightDoomTracker"
    end
  end
end
