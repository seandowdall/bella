import './App.css'

function App() {
  return (
    <main className="shell">
      <section className="hero">
        <p className="eyebrow">Open source AI cost visibility</p>
        <h1>Bella</h1>
        <p className="lede">
          A starting point for tracking AI spend, usage, models, and providers.
        </p>
      </section>

      <section className="cards" aria-label="Project areas">
        <article>
          <h2>API</h2>
          <p>Axum service scaffold with Postgres health checks.</p>
        </article>
        <article>
          <h2>Dashboard</h2>
          <p>Vite and React app ready for cost visibility workflows.</p>
        </article>
        <article>
          <h2>Data</h2>
          <p>SQLx migrations and shared Rust domain types.</p>
        </article>
      </section>
    </main>
  )
}

export default App
