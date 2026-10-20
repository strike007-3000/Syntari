export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: { ink: { 950: "#0b0d12", 900: "#11141b", 800: "#1a1f2b", 700: "#262d3d" }, accent: "#7c9cff" },
      fontFamily: { sans: ["Inter", "system-ui", "sans-serif"] },
    },
  },
  plugins: [],
};
