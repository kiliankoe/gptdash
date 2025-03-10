# GPTdash

This is a game based loosely on [Balderdash](https://en.wikipedia.org/wiki/Balderdash), but with AI. We built it for [Datenspuren 2024](https://www.datenspuren.de/2024/). The main idea is to try to answer a short question while impersonating the AI. Players then vote and try to find the AI answer. You get points for correctly identifying the AI, or for convincing others that you sound more like the AI than the AI itself.

Please don't look all too close at the implementation. It was hastily put together in a few days right before the event, it has many issues, it's super easy to cheat, but it definitely was fun!

## Development

It's a Next.js app (initialized on the T3 stack). Just run the following to get started.

```bash
npm install
npm run dev
```

You will also need to set up your environment by copying `.env.example` to `.env` and at the very least provide an OpenAI API key. The AI model is initialized in `src/server/ai.ts`. The game is much more fun with GPT 3 instead of GPT 4 or better. If/when that's removed from the API, it would probably make sense to use a small offline model with something like Ollama.

## Where to go from here

We intended to transform this into something you can play with your friends whenever, but right now, it doesn't manage more than one game session. There were just some basic preparations to take it in this direction later. We haven't yet gotten around to this, maybe sometime soon. 

Also, it needs some design work lol. It looks as "bad" as it does, as it was supposed to fit with the old-school design of the event. The UX isn't great either when more than 6ish people are submitting answers.
