export async function respondToPrompt(prompt: string) {
  const OPENAI_API_KEY = process.env.OPENAI_API_KEY;
  if (!OPENAI_API_KEY) {
    throw new Error("OPENAI_API_KEY is not set");
  }
  const res = await fetch("https://api.openai.com/v1/chat/completions", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${OPENAI_API_KEY}`,
    },
    body: JSON.stringify({
      model: "gpt-3.5-turbo", // zulässige Optionen sind gpt-4o, gpt-4o-mini, gpt-3.5-turbo
      messages: [
        {
          role: "system",
          content:
            "Bitte antworte in drei kurzen Sätzen auf die folgende Frage oder Aufforderung. Nur drei kurze Sätze, keine Stichpunkte, bitte nur in Fließtext und nicht lang oder umschweifend. Formuliere die kurzen Sätze bitte so wie ein Mensch, der die Antwort innerhalb von 2 Minuten selbst schreibt. Vermeide komplexe Ausdrücke und Formulierungen. Einfach nur drei normale kurze Sätze. Eine ganz kleine Menge Humor ist auch nicht verkehrt. Die Frage lautet:",
        },
        {
          role: "user",
          content: prompt,
        },
      ],
    }),
  });
  if (!res.ok) {
    throw new Error("OpenAI API request failed");
  }
  // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
  const json = await res.json();
  // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-unsafe-member-access
  const answer: string = json.choices[0].message.content;
  return answer;
}
