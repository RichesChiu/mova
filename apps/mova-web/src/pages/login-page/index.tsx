import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { type FormEvent, useState } from 'react'
import { Navigate, useNavigate } from 'react-router-dom'
import {
  ApiError,
  bootstrapAdmin,
  getBootstrapStatus,
  getCurrentUser,
  login,
} from '../../api/client'

export const LoginPage = () => {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const [username, setUsername] = useState('admin')
  const [password, setPassword] = useState('')

  const currentUserQuery = useQuery({
    queryKey: ['current-user'],
    queryFn: getCurrentUser,
    retry: false,
  })

  const bootstrapStatusQuery = useQuery({
    enabled:
      currentUserQuery.isError &&
      currentUserQuery.error instanceof ApiError &&
      currentUserQuery.error.status === 401,
    queryKey: ['bootstrap-status'],
    queryFn: getBootstrapStatus,
    retry: false,
  })

  const authMutation = useMutation({
    mutationFn: async () => {
      const bootstrapRequired = bootstrapStatusQuery.data?.bootstrap_required ?? false
      if (bootstrapRequired) {
        return bootstrapAdmin({ username, password })
      }

      return login({ username, password })
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ['current-user'] })
      await queryClient.invalidateQueries({ queryKey: ['libraries'] })
      navigate('/', { replace: true })
    },
  })

  if (currentUserQuery.isSuccess) {
    return <Navigate replace to="/" />
  }

  if (currentUserQuery.isLoading) {
    return (
      <div className="login-page">
        <section className="login-card">
          <h2>Loading session…</h2>
          <p className="muted">Checking whether you are already signed in.</p>
        </section>
      </div>
    )
  }

  const isUnauthorized =
    currentUserQuery.isError &&
    currentUserQuery.error instanceof ApiError &&
    currentUserQuery.error.status === 401

  if (currentUserQuery.isError && !isUnauthorized) {
    return (
      <div className="login-page">
        <section className="login-card">
          <h2>Session check failed</h2>
          <p className="callout callout--danger">
            {currentUserQuery.error instanceof Error
              ? currentUserQuery.error.message
              : 'Failed to load current user'}
          </p>
        </section>
      </div>
    )
  }

  const bootstrapRequired = bootstrapStatusQuery.data?.bootstrap_required ?? false

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    authMutation.mutate()
  }

  return (
    <div className="login-page">
      <section className="login-card">
        <p className="eyebrow">{bootstrapRequired ? 'Bootstrap' : 'Authentication'}</p>
        <h2>{bootstrapRequired ? 'Create the first admin account' : 'Sign in to Mova'}</h2>

        <form className="stack" onSubmit={handleSubmit}>
          <label className="field">
            <span>Username</span>
            <input
              autoComplete="username"
              onChange={(event) => setUsername(event.target.value)}
              placeholder="admin"
              type="text"
              value={username}
            />
          </label>

          <label className="field">
            <span>Password</span>
            <input
              autoComplete={bootstrapRequired ? 'new-password' : 'current-password'}
              onChange={(event) => setPassword(event.target.value)}
              placeholder="At least 8 characters"
              type="password"
              value={password}
            />
          </label>

          {bootstrapStatusQuery.isLoading ? (
            <p className="muted">Checking bootstrap status…</p>
          ) : null}

          {bootstrapStatusQuery.isError ? (
            <p className="callout callout--danger">
              {bootstrapStatusQuery.error instanceof Error
                ? bootstrapStatusQuery.error.message
                : 'Failed to check bootstrap status'}
            </p>
          ) : null}

          {authMutation.isError ? (
            <p className="callout callout--danger">
              {authMutation.error instanceof Error
                ? authMutation.error.message
                : 'Authentication failed'}
            </p>
          ) : null}

          <button
            className="button button--primary"
            disabled={
              authMutation.isPending ||
              bootstrapStatusQuery.isLoading ||
              username.trim().length === 0 ||
              password.length === 0
            }
            type="submit"
          >
            {authMutation.isPending
              ? bootstrapRequired
                ? 'Creating admin…'
                : 'Signing in…'
              : bootstrapRequired
                ? 'Create Admin'
                : 'Sign In'}
          </button>
        </form>
      </section>
    </div>
  )
}
