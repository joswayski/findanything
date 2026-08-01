import { useEffect, useState } from "react";

const REPO_URL = "https://github.com/joswayski/findanything";
const X_URL = "https://x.com/josevalerio";

const relativeTimeFormatter = new Intl.RelativeTimeFormat("en", {
  numeric: "always",
});

export default function Home() {
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    const interval = window.setInterval(() => setNow(Date.now()), 60_000);
    return () => window.clearInterval(interval);
  }, []);

  return (
    <div className="site-shell">
      <main className="site-main">
        <section className="hero">
          <p className="status-pill">
            <span aria-hidden="true" />
            Work in progress
          </p>

          <div className="brand-row">
            <span className="brand-symbol" aria-hidden="true">
              <FindAnythingIcon />
            </span>
            <h1>Find Anything</h1>
          </div>

          <p className="tagline">
            A fast, local-first desktop launcher that understands what you mean and learns what you
            choose. Built by{" "}
            <a href={X_URL} target="_blank" rel="noreferrer">
              Jose Valerio
            </a>
            .
          </p>

          <a className="github-button" href={REPO_URL} target="_blank" rel="noreferrer">
            <GitHubIcon />
            View on GitHub
          </a>
        </section>

        <section className="recent" aria-labelledby="recent-heading">
          <h2 id="recent-heading">{__RECENT_SECTION_TITLE__}</h2>

          <ol className="timeline">
            {__RECENT_ITEMS__.map((item) => (
              <li key={item.key}>
                <span className="timeline-dot" aria-hidden="true" />
                <a href={item.url} target="_blank" rel="noreferrer">
                  {item.title}
                </a>
                <p>
                  <time dateTime={item.occurredAt}>{formatRelativeTime(item.occurredAt, now)}</time>
                  <span aria-hidden="true"> · </span>
                  {item.detail}
                </p>
              </li>
            ))}
          </ol>
        </section>
      </main>
    </div>
  );
}

function formatRelativeTime(date: string, now: number) {
  const secondsFromNow = (new Date(date).getTime() - now) / 1_000;
  const divisions = [
    { amount: 60, unit: "second" },
    { amount: 60, unit: "minute" },
    { amount: 24, unit: "hour" },
    { amount: 7, unit: "day" },
    { amount: 4.345, unit: "week" },
    { amount: 12, unit: "month" },
    { amount: Number.POSITIVE_INFINITY, unit: "year" },
  ] as const;

  let duration = secondsFromNow;
  for (const division of divisions) {
    if (Math.abs(duration) < division.amount) {
      return relativeTimeFormatter.format(Math.round(duration), division.unit);
    }
    duration /= division.amount;
  }

  return relativeTimeFormatter.format(0, "second");
}

function FindAnythingIcon() {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" aria-hidden="true">
      <circle cx="10.5" cy="10.5" r="5.75" />
      <path d="m14.8 14.8 4.45 4.45" />
      <path className="search-spark" d="M18.25 4.25v3.5M16.5 6h3.5" />
    </svg>
  );
}

function GitHubIcon() {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M12 2C6.477 2 2 6.486 2 12.021c0 4.425 2.865 8.18 6.839 9.504.5.092.682-.217.682-.483 0-.237-.009-.866-.013-1.7-2.782.605-3.369-1.343-3.369-1.343-.454-1.158-1.11-1.467-1.11-1.467-.908-.621.069-.609.069-.609 1.004.071 1.532 1.032 1.532 1.032.892 1.53 2.341 1.088 2.91.833.091-.647.35-1.088.636-1.339-2.22-.253-4.555-1.113-4.555-4.952 0-1.093.39-1.988 1.029-2.688-.103-.253-.446-1.272.098-2.65 0 0 .84-.27 2.75 1.026A9.564 9.564 0 0 1 12 6.844c.85.004 1.705.115 2.504.337 1.909-1.296 2.747-1.027 2.747-1.027.546 1.379.202 2.398.1 2.651.64.7 1.028 1.595 1.028 2.688 0 3.848-2.339 4.695-4.566 4.944.359.31.678.92.678 1.855 0 1.338-.012 2.419-.012 2.748 0 .268.18.58.688.481A10.02 10.02 0 0 0 22 12.021C22 6.486 17.523 2 12 2Z" />
    </svg>
  );
}
