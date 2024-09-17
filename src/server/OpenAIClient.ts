import OpenAI from "openai";

function createClient() {
  const OPENAI_API_KEY = process.env.OPENAI_API_KEY;
  if (!OPENAI_API_KEY) {
    throw new Error("OPENAI_API_KEY is not set");
  }
  return new OpenAI({ apiKey: OPENAI_API_KEY });
}

let openAIClient: OpenAI | undefined;

export function getOpenAIClient() {
  if (!openAIClient) {
    openAIClient = createClient();
  }
  return openAIClient;
}
