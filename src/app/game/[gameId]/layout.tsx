export default async function GameLayout({
  params,
  children,
}: Readonly<{ params: { gameId: string }; children: React.ReactNode }>) {
  return (
    <>
      <h1 className="absolute left-8 top-4">{params.gameId}</h1>
      <div className="flex h-screen flex-col items-center justify-center">
        {children}
      </div>
    </>
  );
}
