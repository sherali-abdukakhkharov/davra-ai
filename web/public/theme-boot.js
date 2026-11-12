// Runs before first paint so the page never flashes the wrong theme or language.
(function () {
  var d = document.documentElement;
  var theme = null;
  var lang = null;
  try {
    theme = localStorage.getItem("davra-theme");
    lang = localStorage.getItem("davra-lang");
  } catch (e) {}
  if (theme !== "light" && theme !== "dark") {
    theme = window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }
  d.setAttribute("data-theme", theme);
  d.style.colorScheme = theme;
  if (lang === "uz") d.setAttribute("lang", "uz");
})();
