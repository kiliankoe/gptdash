import "~/styles/globals.css";

import { GeistSans } from "geist/font/sans";
import { type Metadata } from "next";
import { getGameState } from "~/server/actions";

export const metadata: Metadata = {
  title: "GPTDash",
  description: "",
  icons: [{ rel: "icon", url: "/favicon.ico" }],
};

export default async function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en" className={`${GeistSans.variable}`}>
      <head>
        <link
          rel="stylesheet"
          media="screen"
          href="https://fontlibrary.org//face/xolonium"
          type="text/css"
        />
      </head>
      <body>
        {children}
        {process.env.NODE_ENV === "development" && (
          <footer>
            <code className="text-[10px] text-white">
              {JSON.stringify(await getGameState("ds24"), null, 2)}
            </code>
          </footer>
        )}
      </body>
    </html>
  );
}
