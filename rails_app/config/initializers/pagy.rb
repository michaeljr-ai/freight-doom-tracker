# frozen_string_literal: true

require 'pagy/extras/metadata'

# Default items per page
Pagy::DEFAULT[:items] = 25

# How many page links around the current page
Pagy::DEFAULT[:size] = [1, 4, 4, 1]
