export default async function GameLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <>
      <div className="flex h-screen flex-col items-center justify-center">
        {children}
      </div>
    </>
  );
}
