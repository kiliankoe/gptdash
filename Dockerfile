FROM node:latest
WORKDIR /app

COPY . .

RUN npx next telemetry disable
RUN npm install --verbose

ENV DATABASE_URL=file:./db.sqlite

RUN npm run build

EXPOSE 3000

CMD ["npm", "start"]
