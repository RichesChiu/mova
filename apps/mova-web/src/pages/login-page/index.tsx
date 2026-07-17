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
import { useI18n } from '../../i18n'
import { USER_ACCOUNT_MAX_LENGTH } from '../../lib/user-account'

export const LoginPage = () => {
  const { l } = useI18n()
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
          <h2>{l('Loading session…')}</h2>
          <p className="muted">{l('Checking whether you are already signed in.')}</p>
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
          <h2>{l('Session check failed')}</h2>
          <p className="callout callout--danger">
            {currentUserQuery.error instanceof Error
              ? currentUserQuery.error.message
              : l('Failed to load current user')}
          </p>
        </section>
      </div>
    )
  }

  const bootstrapRequired = bootstrapStatusQuery.data?.bootstrap_required ?? false
  const formErrorMessage = authMutation.isError
    ? authMutation.error instanceof Error
      ? authMutation.error.message
      : l('Authentication failed')
    : bootstrapStatusQuery.isError
      ? bootstrapStatusQuery.error instanceof Error
        ? bootstrapStatusQuery.error.message
        : l('Failed to check bootstrap status')
      : null

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    authMutation.mutate()
  }

  return (
    <div className="login-page">
      <section className="login-card">
        <p className="eyebrow">{bootstrapRequired ? l('Bootstrap') : l('Authentication')}</p>
        <h2>{bootstrapRequired ? l('Create the first admin account') : l('Sign in to Mova')}</h2>

        <form className="stack" onSubmit={handleSubmit}>
          <label className="field">
            <span>{l('Account')}</span>
            <input
              autoComplete="username"
              maxLength={USER_ACCOUNT_MAX_LENGTH}
              onChange={(event) => {
                setUsername(event.target.value)
                authMutation.reset()
              }}
              placeholder="admin"
              spellCheck={false}
              type="text"
              value={username}
            />
          </label>

          <label className="field">
            <span>{l('Password')}</span>
            <input
              aria-describedby={formErrorMessage ? 'login-password-error' : undefined}
              aria-invalid={authMutation.isError || undefined}
              autoComplete={bootstrapRequired ? 'new-password' : 'current-password'}
              onChange={(event) => {
                setPassword(event.target.value)
                authMutation.reset()
              }}
              placeholder={l('At least 8 characters')}
              type="password"
              value={password}
            />
            {formErrorMessage ? (
              <small className="login-card__field-error" id="login-password-error" role="alert">
                {formErrorMessage}
              </small>
            ) : null}
          </label>

          {bootstrapStatusQuery.isLoading ? (
            <p className="muted">{l('Checking bootstrap status…')}</p>
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
                ? l('Creating admin…')
                : l('Signing in…')
              : bootstrapRequired
                ? l('Create Admin')
                : l('Sign In')}
          </button>
        </form>
      </section>
    </div>
  )
}
