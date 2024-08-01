"use client";

import { useState } from "react";

const RegisterForm = () => {
  const [name, setName] = useState<string | null>(null);

  return (
    <form action="/api/register" method="post" className="flex flex-col">
      <input
        type="text"
        name="name"
        placeholder="Dein Name"
        value={name ?? ""}
        onChange={(e) => setName(e.target.value)}
        className="border-orange text-orange bg-bg-blue w-full border-2 p-2 text-center"
      />
      <button
        type="submit"
        disabled={!name}
        className="text-green hover:text-orange px-4 py-2 font-bold disabled:text-gray-500"
      >
        Los geht&apos;s!
      </button>
    </form>
  );
};

export default RegisterForm;
