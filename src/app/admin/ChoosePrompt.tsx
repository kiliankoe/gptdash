import { useState } from "react";
import Button from "../components/Button";

export default function ChoosePrompt() {
  const [prompt, setPrompt] = useState<string>("");

  const handleSubmit = () => {
    fetch("/api/game/ds24/status", {
      method: "POST",
      body: JSON.stringify({ status: "prompting", prompt }),
    }).catch((e) => console.error(e));
  };

  return (
    <div>
      <textarea
        placeholder="Prompt"
        value={prompt}
        onChange={(e) => setPrompt(e.target.value)}
        className="border-2 border-orange p-2 text-center text-orange"
      />
      <Button onClick={handleSubmit}>Prompt setzen</Button>
    </div>
  );
}
