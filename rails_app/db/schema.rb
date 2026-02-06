# This file is auto-generated from the current state of the database. Instead
# of editing this file, please use the migrations feature of Active Record to
# incrementally modify your database, and then regenerate this schema definition.
#
# This file is the source Rails uses to define your schema when running `bin/rails
# db:schema:load`. When creating a new database, `bin/rails db:schema:load` tends to
# be faster and is potentially less error prone than running all of your
# migrations from scratch. Old migrations may fail to apply correctly if those
# migrations use external dependencies or application code.
#
# It's strongly recommended that you check this file into your version control system.

ActiveRecord::Schema[7.2].define(version: 3) do
  create_table "alert_subscriptions", force: :cascade do |t|
    t.string "email", null: false
    t.string "carrier_type_filter"
    t.string "keyword_filter"
    t.boolean "active", default: true, null: false
    t.datetime "created_at", null: false
    t.datetime "updated_at", null: false
    t.index ["active"], name: "index_alert_subscriptions_on_active"
    t.index ["carrier_type_filter"], name: "index_alert_subscriptions_on_carrier_type_filter"
    t.index ["email"], name: "index_alert_subscriptions_on_email"
  end

  create_table "bankruptcy_events", force: :cascade do |t|
    t.string "company_name", null: false
    t.string "dot_number"
    t.string "mc_number"
    t.date "filing_date"
    t.string "court"
    t.integer "chapter"
    t.string "source", default: "pacer", null: false
    t.float "confidence_score", default: 0.0
    t.json "raw_data", default: {}
    t.datetime "detected_at", default: -> { "CURRENT_TIMESTAMP" }, null: false
    t.string "status", default: "new", null: false
    t.datetime "created_at", null: false
    t.datetime "updated_at", null: false
    t.index ["chapter"], name: "index_bankruptcy_events_on_chapter"
    t.index ["company_name"], name: "index_bankruptcy_events_on_company_name"
    t.index ["detected_at"], name: "index_bankruptcy_events_on_detected_at"
    t.index ["dot_number"], name: "index_bankruptcy_events_on_dot_number"
    t.index ["filing_date"], name: "index_bankruptcy_events_on_filing_date"
    t.index ["source", "status"], name: "index_bankruptcy_events_on_source_and_status"
    t.index ["source"], name: "index_bankruptcy_events_on_source"
    t.index ["status"], name: "index_bankruptcy_events_on_status"
  end

  create_table "carriers", force: :cascade do |t|
    t.string "name", null: false
    t.string "dot_number"
    t.string "mc_number"
    t.string "carrier_type", default: "carrier", null: false
    t.string "status", default: "active", null: false
    t.string "authority_status"
    t.datetime "last_checked_at"
    t.datetime "created_at", null: false
    t.datetime "updated_at", null: false
    t.index ["authority_status"], name: "index_carriers_on_authority_status"
    t.index ["carrier_type"], name: "index_carriers_on_carrier_type"
    t.index ["dot_number"], name: "index_carriers_on_dot_number", unique: true
    t.index ["mc_number"], name: "index_carriers_on_mc_number"
    t.index ["name"], name: "index_carriers_on_name"
    t.index ["status"], name: "index_carriers_on_status"
  end
end
