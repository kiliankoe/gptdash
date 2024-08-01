import RegisterForm from "./RegisterForm";

export default function HomePage() {
  return (
    <main className="flex min-h-screen items-center justify-center">
      <div className="border-orange flex flex-col items-center gap-y-6 border-4 px-4 py-16">
        <div className="flex flex-col items-end">
          <h1 className="text-6xl font-bold sm:text-[5rem]">GPTDash</h1>
          <a href="https://datenspuren.de/2024/">
            <h2 className="text-orange -translate-y-3 text-xs underline">
              Datenspuren 2024
            </h2>
          </a>
        </div>
        <p className="m-2 max-w-[400px] text-center text-lg sm:m-0">
          Bist du die ultimative LLM-Imitation?! Täusche die anderen und finde
          die korrekte Antwort!
        </p>
        <RegisterForm />
      </div>
    </main>
  );
}
