FROM node:24-bookworm-slim

RUN npm install -g pnpm@11.0.8

WORKDIR /app
