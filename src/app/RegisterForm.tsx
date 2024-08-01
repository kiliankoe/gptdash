"use client";

import { useState } from "react";
import Button from "./components/Button";

const RegisterForm = () => {
  const [name, setName] = useState<string | null>(null);

  return (
    <form
      action="/api/register"
      method="post"
      className="flex flex-col gap-y-2"
    >
      <input
        type="text"
        name="name"
        placeholder="Dein Name"
        value={name ?? ""}
        onChange={(e) => setName(e.target.value)}
        className="border-orange text-orange bg-bg-blue w-full border-2 p-2 text-center"
      />
      <Button type="submit" disabled={!name}>
        Los geht&apos;s!
      </Button>
    </form>
  );
};

export default RegisterForm;
