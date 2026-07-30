import { Component, type ErrorInfo, type ReactNode } from 'react'

interface RouteLoadErrorBoundaryProps {
  children: ReactNode
  description: string
  onReload: () => void
  reloadLabel: string
  resetKey: string
  title: string
}

interface RouteLoadErrorBoundaryState {
  error: Error | null
}

export class RouteLoadErrorBoundary extends Component<
  RouteLoadErrorBoundaryProps,
  RouteLoadErrorBoundaryState
> {
  state: RouteLoadErrorBoundaryState = {
    error: null,
  }

  static getDerivedStateFromError(error: Error): RouteLoadErrorBoundaryState {
    return { error }
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('A lazy route could not be loaded.', error, info)
  }

  componentDidUpdate(previousProps: RouteLoadErrorBoundaryProps) {
    if (previousProps.resetKey !== this.props.resetKey && this.state.error) {
      this.setState({ error: null })
    }
  }

  render() {
    if (!this.state.error) {
      return this.props.children
    }

    return (
      <div className="app-route-fallback" role="alert">
        <section className="app-route-fallback__error">
          <h2>{this.props.title}</h2>
          <p>{this.props.description}</p>
          <button className="button button--primary" onClick={this.props.onReload} type="button">
            {this.props.reloadLabel}
          </button>
        </section>
      </div>
    )
  }
}
