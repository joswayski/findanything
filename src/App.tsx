import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";

type ResultKind = "application" | "system_action" | "file";

type SearchResult = {
  id: string;
  kind: ResultKind;
  title: string;
  subtitle: string;
  score: number;
  reason: string;
};

type SearchResponse = {
  results: SearchResult[];
  semanticStatus: "warming" | "ready" | "unavailable";
  semanticMessage?: string;
};

const EMPTY_RESPONSE: SearchResponse = {
  results: [],
  semanticStatus: "warming",
};

function ResultIcon({ kind, title }: { kind: ResultKind; title: string }) {
  if (kind === "system_action") {
    return <span className="result-icon action-icon">⌁</span>;
  }
  if (kind === "file") {
    return <span className="result-icon file-icon">⌑</span>;
  }

  return (
    <span className="result-icon app-icon" aria-hidden="true">
      {title.slice(0, 1).toLocaleUpperCase()}
    </span>
  );
}

function semanticLabel(status: SearchResponse["semanticStatus"]) {
  if (status === "ready") return "Semantic ready";
  if (status === "unavailable") return "Keyword mode";
  return "Preparing index";
}

export default function App() {
  const [query, setQuery] = useState("");
  const [response, setResponse] = useState<SearchResponse>(EMPTY_RESPONSE);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [error, setError] = useState<string>();
  const inputRef = useRef<HTMLInputElement>(null);
  const requestNumber = useRef(0);

  useEffect(() => {
    const currentRequest = ++requestNumber.current;
    const timer = window.setTimeout(async () => {
      try {
        const next = await invoke<SearchResponse>("search", { query });
        if (requestNumber.current !== currentRequest) return;
        setResponse(next);
        setSelectedIndex((index) => Math.min(index, Math.max(0, next.results.length - 1)));
        setError(undefined);
      } catch (caught) {
        if (requestNumber.current !== currentRequest) return;
        setError(String(caught));
      }
    }, 32);

    return () => window.clearTimeout(timer);
  }, [query]);

  useEffect(() => {
    inputRef.current?.focus();
    const unlisten = listen("launcher-shown", () => {
      inputRef.current?.focus();
      inputRef.current?.select();
    });
    return () => {
      void unlisten.then((dispose) => dispose());
    };
  }, []);

  useEffect(() => {
    if (response.semanticStatus !== "warming") return;

    let cancelled = false;
    const timer = window.setInterval(async () => {
      const currentRequest = ++requestNumber.current;
      try {
        const next = await invoke<SearchResponse>("search", { query });
        if (cancelled || requestNumber.current !== currentRequest) return;
        setResponse(next);
      } catch {
        // The primary search effect owns user-facing errors. This poll only
        // refreshes the background model status.
      }
    }, 500);

    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [query, response.semanticStatus]);

  async function activate(result: SearchResult) {
    try {
      await invoke("activate_result", { id: result.id, query });
      await invoke("hide_launcher");
    } catch (caught) {
      setError(String(caught));
    }
  }

  function onKeyDown(event: React.KeyboardEvent<HTMLInputElement>) {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setSelectedIndex((index) => Math.min(index + 1, response.results.length - 1));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setSelectedIndex((index) => Math.max(index - 1, 0));
    } else if (event.key === "Enter") {
      const result = response.results[selectedIndex];
      if (result) void activate(result);
    } else if (event.key === "Escape") {
      void invoke("hide_launcher");
    }
  }

  return (
    <main className="launcher-shell">
      <section className="launcher" aria-label="Find Anything">
        <div className="search-row">
          <svg className="search-icon" viewBox="0 0 24 24" aria-hidden="true">
            <circle cx="10.75" cy="10.75" r="6.5" />
            <path d="m15.6 15.6 4.2 4.2" />
          </svg>
          <input
            ref={inputRef}
            value={query}
            onChange={(event) => {
              setQuery(event.target.value);
              setSelectedIndex(0);
            }}
            onKeyDown={onKeyDown}
            placeholder="Find anything…"
            aria-label="Search applications, settings, and files"
            autoComplete="off"
            spellCheck={false}
          />
          {query && (
            <button className="clear-button" onClick={() => setQuery("")} aria-label="Clear search">
              ×
            </button>
          )}
        </div>

        <div className="results-heading">
          <span>{query ? "Best matches" : "Apps & actions"}</span>
        </div>

        <div className="results" aria-label="Search results">
          {response.results.map((result, index) => (
            <button
              key={result.id}
              type="button"
              className={`result ${index === selectedIndex ? "selected" : ""}`}
              aria-current={index === selectedIndex ? "true" : undefined}
              onMouseEnter={() => setSelectedIndex(index)}
              onClick={() => void activate(result)}
            >
              <ResultIcon kind={result.kind} title={result.title} />
              <span className="result-copy">
                <span className="result-title-line">
                  <span className="result-title">{result.title}</span>
                  <span className="result-subtitle">{result.subtitle}</span>
                </span>
                <span className="result-reason">{result.reason}</span>
              </span>
              <span className="open-hint">Open&nbsp; ↵</span>
            </button>
          ))}

          {!error && response.results.length === 0 && (
            <div className="empty-state">
              <span className="empty-mark">⌕</span>
              <p>No local matches yet.</p>
              <span>Try an app, a setting, or a filename.</span>
            </div>
          )}
          {error && (
            <div className="empty-state error-state">
              <p>Search hit a snag.</p>
              <span>{error}</span>
            </div>
          )}
        </div>

        <footer>
          <span
            className={`semantic-status footer-status ${response.semanticStatus}`}
            title={response.semanticMessage}
          >
            <span className="status-dot" />
            {semanticLabel(response.semanticStatus)}
          </span>
          <span className="key-hints">
            <kbd>↑</kbd><kbd>↓</kbd> Navigate <kbd>↵</kbd> Open <kbd>esc</kbd> Close
          </span>
        </footer>
      </section>
    </main>
  );
}
