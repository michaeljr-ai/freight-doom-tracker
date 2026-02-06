class CreateCarriers < ActiveRecord::Migration[7.1]
  def change
    create_table :carriers do |t|
      t.string :name, null: false
      t.string :dot_number
      t.string :mc_number
      t.string :carrier_type, null: false, default: "carrier"
      t.string :status, null: false, default: "active"
      t.string :authority_status
      t.datetime :last_checked_at

      t.timestamps
    end

    add_index :carriers, :name
    add_index :carriers, :dot_number, unique: true
    add_index :carriers, :mc_number
    add_index :carriers, :carrier_type
    add_index :carriers, :status
    add_index :carriers, :authority_status
  end
end
