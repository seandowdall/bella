import { useEffect, useState } from 'react'
import './App.css'

type User = {
  id: string
  github_login: string
  name: string | null
  avatar_url: string | null
}

const apiBaseUrl = import.meta.env.VITE_BELLA_API_BASE_URL ?? '/api'

function App() {
  const [user, setUser] = useState<User | null>(null)
  const [loading, setLoading] = useState(true)
  const cliSuccess = window.location.pathname === '/auth/cli/success'

  useEffect(() => {
    fetch(`${apiBaseUrl}/v1/me`, { credentials: 'include' })
      .then((response) => (response.ok ? response.json() : null))
      .then(setUser)
      .finally(() => setLoading(false))
  }, [])

  const login = () => {
    const returnTo = `${window.location.origin}/`
    window.location.assign(
      `${apiBaseUrl}/v1/auth/github/start?return_to=${encodeURIComponent(returnTo)}`,
    )
  }

  const logout = async () => {
    await fetch(`${apiBaseUrl}/v1/auth/logout`, {
      method: 'POST',
      credentials: 'include',
    })
    setUser(null)
  }

  if (cliSuccess) {
    return (
      <main className="shell">
        <section className="hero auth-card">
          <p className="eyebrow">Bella CLI</p>
          <h1>Login complete</h1>
          <p className="lede">You can close this tab and return to your terminal.</p>
        </section>
      </main>
    )
  }

  return (
    <main className="shell">
      <section className="hero">
        <div className="account">
          {!loading && user ? (
            <>
              {user.avatar_url && (
                <img src={user.avatar_url} alt="" className="avatar" />
              )}
              <span>@{user.github_login}</span>
              <button type="button" className="secondary" onClick={logout}>
                Log out
              </button>
            </>
          ) : !loading ? (
            <button type="button" onClick={login}>
              Log in with GitHub
            </button>
          ) : null}
        </div>
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
          <p>
            {user
              ? `Authenticated as ${user.name ?? user.github_login}.`
              : 'Log in with GitHub to access your Bella workspace.'}
          </p>
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
