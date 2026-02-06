class CreateAlertSubscriptions < ActiveRecord::Migration[7.1]
  def change
    create_table :alert_subscriptions do |t|
      t.string :email, null: false
      t.string :carrier_type_filter
      t.string :keyword_filter
      t.boolean :active, null: false, default: true

      t.timestamps
    end

    add_index :alert_subscriptions, :email
    add_index :alert_subscriptions, :active
    add_index :alert_subscriptions, :carrier_type_filter
  end
end
