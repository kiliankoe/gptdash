"use client";

import { useRouter } from "next/navigation";
import { useState } from "react";
import Button from "./components/Button";

const RegisterForm = () => {
  const [name, setName] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const router = useRouter();

  const handleSubmit = async () => {
    if (!name) return;
    const res = await fetch("/api/game/ds24/players", {
      method: "POST",
      body: name,
    });
    if (!res.ok) {
      setError(await res.text());
      return;
    }
    // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
    const json = await res.json();
    // eslint-disable-next-line @typescript-eslint/no-unsafe-argument, @typescript-eslint/no-unsafe-member-access
    window.localStorage.setItem("playerId", json.playerId);
    setName(null);
    router.push(`/game/ds24`);
  };

  const handleKeyDown = async (
    event: React.KeyboardEvent<HTMLInputElement>,
  ) => {
    if (event.key === "Enter") {
      await handleSubmit();
    }
  };

  return (
    <div className="flex flex-col gap-y-2">
      <input
        type="text"
        name="name"
        placeholder="Dein Name"
        value={name ?? ""}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={handleKeyDown}
        className="w-full border-2 border-orange bg-bg-blue p-2 text-center text-orange"
      />
      <Button disabled={!name} onClick={() => handleSubmit()}>
        Los geht&apos;s!
      </Button>
      {error && <p className="text-red-500">{error}</p>}
    </div>
  );
};

export default RegisterForm;
