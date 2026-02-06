FROM ruby:3.2-slim-bookworm

RUN apt-get update && apt-get install -y \
    build-essential \
    libpq-dev \
    libsqlite3-dev \
    nodejs \
    git \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Gemfile Gemfile.lock ./
RUN bundle install --without development test --jobs 4 --retry 3

COPY . .

RUN bundle exec rails assets:precompile 2>/dev/null || true

ENV RAILS_ENV=production
ENV RAILS_LOG_TO_STDOUT=true
ENV PORT=3000

EXPOSE 3000

CMD ["bash", "-c", "bundle exec rails db:migrate 2>/dev/null; bundle exec puma -C config/puma.rb"]
