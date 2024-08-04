"use client";

import { useRouter } from "next/navigation";
import { useState } from "react";
import { addPlayer } from "../server/actions";
import Button from "./components/Button";

const RegisterForm = () => {
  const [name, setName] = useState<string | null>(null);
  const router = useRouter();

  const handleSubmit = async () => {
    if (!name) return;
    await addPlayer(name, "ds24");
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
        className="border-orange text-orange bg-bg-blue w-full border-2 p-2 text-center"
      />
      <Button disabled={!name} onClick={() => handleSubmit()}>
        Los geht&apos;s!
      </Button>
    </div>
  );
};

export default RegisterForm;
