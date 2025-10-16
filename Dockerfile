# Build stage - use Debian-based Node for glibc compatibility
FROM node:24-slim AS builder

RUN apt-get update && apt-get install -y make binutils && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY package.json yarn.lock
RUN yarn install --frozen-lockfile
COPY . .
RUN yarn bundle
RUN make

# Final stage - minimal Debian image with necessary runtime libraries
FROM debian:bookworm-slim

WORKDIR /app
COPY --from=builder /app/dist/hl /app/hl

CMD ["/app/hl"]