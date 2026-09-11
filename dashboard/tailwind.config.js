/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      colors: {
        hikma: {
          bg: "#1A0E06",
          "bg-2": "#241408",
          ember: "#E8760D",
          "ember-light": "#F4A94D",
          paper: "#F5EFE8",
          muted: "#A89484",
          steel: "#9FB3C8",
        },
      },
      fontFamily: {
        display: ["Oswald", "sans-serif"],
        body: ["Inter", "sans-serif"],
        mono: ["JetBrains Mono", "monospace"],
      },
    },
  },
  plugins: [],
};
