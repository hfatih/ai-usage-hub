const cardSelector = ".summary-strip, .provider-card, .glass-panel, .account-card, .dashboard-account-row, .sidebar-status, .detected-app, .privacy-note, .ready-card";

function starField(width: number, height: number) {
  const stars: string[] = [];
  const cellWidth = 72;
  const cellHeight = 75;
  const columns = Math.ceil(width / cellWidth);
  const rows = Math.ceil(height / cellHeight);

  for (let row = 0; row < rows; row++) {
    for (let column = 0; column < columns; column++) {
      const roll = Math.random();
      if (roll > 0.52) continue;

      const x = Math.round((column + 0.15 + Math.random() * 0.7) * width / columns);
      const y = Math.round((row + 0.15 + Math.random() * 0.7) * height / rows);

      if (roll < 0.28) {
        // Classic 4-pointed sparkle star (medium / prominent)
        const size = Math.round(3.5 + Math.random() * 3.5);
        const opacity = (0.13 + Math.random() * 0.22).toFixed(2);
        stars.push(`<path d="M${x} ${y - size}L${x + 1} ${y - 1} ${x + size} ${y} ${x + 1} ${y + 1} ${x} ${y + size} ${x - 1} ${y + 1} ${x - size} ${y} ${x - 1} ${y - 1}Z" opacity="${opacity}"/>`);
      } else {
        // Delicate mini sparkle
        const size = Math.round(2 + Math.random() * 1.5);
        const opacity = (0.11 + Math.random() * 0.18).toFixed(2);
        stars.push(`<path d="M${x} ${y - size}L${x + 0.8} ${y - 0.8} ${x + size} ${y} ${x + 0.8} ${y + 0.8} ${x} ${y + size} ${x - 0.8} ${y + 0.8} ${x - size} ${y} ${x - 0.8} ${y - 0.8}Z" opacity="${opacity}"/>`);
      }
    }
  }

  const dots = Math.round((width * height * 72) / (1200 * 840));
  for (let dot = 0; dot < dots; dot++) {
    const x = Math.round(Math.random() * width);
    const y = Math.round(Math.random() * height);
    const r = (0.8 + Math.random() * 1.2).toFixed(1);
    const opacity = (0.10 + Math.random() * 0.18).toFixed(2);
    stars.push(`<circle cx="${x}" cy="${y}" r="${r}" opacity="${opacity}"/>`);
  }

  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}"><g fill="#ffc3d7">${stars.join("")}</g></svg>`;
  return `url("data:image/svg+xml,${encodeURIComponent(svg)}")`;
}

export function setStarFields() {
  const root = document.getElementById("root")!;
  const watched = new Set<HTMLElement>();
  const resize = new ResizeObserver((entries) => {
    for (const entry of entries) {
      const card = entry.target as HTMLElement;
      const { width, height } = card.getBoundingClientRect();
      if (width && height) card.style.setProperty("--card-stars", starField(Math.ceil(width), Math.ceil(height)));
    }
  });
  const observeCards = () => {
    for (const card of watched) {
      if (root.contains(card)) continue;
      resize.unobserve(card);
      watched.delete(card);
    }
    root.querySelectorAll<HTMLElement>(cardSelector).forEach((card) => {
      if (watched.has(card)) return;
      watched.add(card);
      resize.observe(card);
    });
  };
  new MutationObserver(observeCards).observe(root, { childList: true, subtree: true });
  observeCards();
}
