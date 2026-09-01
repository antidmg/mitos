(() => {
  const container = document.querySelector(".toc-container");
  const toc = document.getElementById("page-toc");
  const main = document.querySelector("#mdbook-content main");

  if (!container || !toc || !main) {
    return;
  }

  const headings = Array.from(main.querySelectorAll("h2, h3"));
  if (headings.length === 0) {
    container.hidden = true;
    return;
  }

  const links = headings.map((heading) => {
    const header = heading.querySelector(".header");
    const link = document.createElement("a");
    link.href = header?.getAttribute("href") ?? `#${heading.id}`;
    link.textContent = header?.textContent ?? heading.textContent;
    link.className = `toc-level-${heading.tagName.slice(1)}`;
    toc.appendChild(link);
    return link;
  });

  let frame;
  const updateActiveLink = () => {
    frame = undefined;
    const offset = 96;
    let active = 0;

    for (let index = 0; index < headings.length; index += 1) {
      if (headings[index].getBoundingClientRect().top > offset) {
        break;
      }
      active = index;
    }

    links.forEach((link, index) => {
      link.classList.toggle("active", index === active);
      if (index === active) {
        link.setAttribute("aria-current", "location");
      } else {
        link.removeAttribute("aria-current");
      }
    });
  };

  const scheduleUpdate = () => {
    if (frame === undefined) {
      frame = requestAnimationFrame(updateActiveLink);
    }
  };

  document.addEventListener("scroll", scheduleUpdate, { passive: true });
  document.querySelector(".content")?.addEventListener("scroll", scheduleUpdate, {
    passive: true,
  });
  window.addEventListener("resize", scheduleUpdate, { passive: true });
  updateActiveLink();
})();
