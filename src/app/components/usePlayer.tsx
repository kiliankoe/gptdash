import { useEffect, useState } from "react";

export default function usePlayer() {
  const [playerId, setPlayerId] = useState(() =>
    window.localStorage.getItem("playerId"),
  );

  useEffect(() => {
    const handleStorageChange = () => {
      setPlayerId(window.localStorage.getItem("playerId"));
    };

    window.addEventListener("storage", handleStorageChange);

    return () => {
      window.removeEventListener("storage", handleStorageChange);
    };
  }, []);

  return playerId;
}
