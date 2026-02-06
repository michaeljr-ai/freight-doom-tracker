class CreateBankruptcyEvents < ActiveRecord::Migration[7.1]
  def change
    create_table :bankruptcy_events do |t|
      t.string :company_name, null: false
      t.string :dot_number
      t.string :mc_number
      t.date :filing_date
      t.string :court
      t.integer :chapter
      t.string :source, null: false, default: "pacer"
      t.float :confidence_score, default: 0.0
      t.json :raw_data, default: {}
      t.datetime :detected_at, null: false, default: -> { "CURRENT_TIMESTAMP" }
      t.string :status, null: false, default: "new"

      t.timestamps
    end

    add_index :bankruptcy_events, :company_name
    add_index :bankruptcy_events, :dot_number
    add_index :bankruptcy_events, :detected_at
    add_index :bankruptcy_events, :source
    add_index :bankruptcy_events, :status
    add_index :bankruptcy_events, :chapter
    add_index :bankruptcy_events, :filing_date
    add_index :bankruptcy_events, [:source, :status]
  end
end
