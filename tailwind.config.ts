import { type Config } from "tailwindcss";
import { fontFamily } from "tailwindcss/defaultTheme";

export default {
  content: ["./src/**/*.tsx"],
  theme: {
    extend: {
      fontFamily: {
        sans: ["var(--font-geist-sans)", ...fontFamily.sans],
        xolonium: ["Xolonium", ...fontFamily.sans],
      },
      colors: {
        green: "#629103",
        orange: "#ffab00",
        "bg-blue": "#07006c",
      },
    },
  },
  plugins: [],
} satisfies Config;
