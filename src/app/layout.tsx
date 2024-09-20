import { GeistSans } from "geist/font/sans";
import type { Metadata } from "next";
import "~/styles/globals.css";
import ClientLayout from "./ClientLayout";
import DevFooter from "./components/DevFooter";

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
      <body className="m-2">
        <ClientLayout>
          {children}
          {process.env.NODE_ENV === "development" && <DevFooter />}
        </ClientLayout>
      </body>
    </html>
  );
}
