import { TurnstileWidget, useTurnstile, mount } from 'webrify-turnstile'
import { useEffect, useRef } from 'react'

const sectionStyle = {
  marginBottom: 32,
  padding: 20,
  border: '1px solid #ddd',
  borderRadius: 8,
  background: '#fafafa',
}

const hintStyle = { fontSize: 13, color: '#888', marginTop: 0, marginBottom: 12 }

/** 自定义 UI — 基于 useTurnstile hook，完全控制渲染 */
function CustomCaptcha() {
  const { status, errorMessage, verify, reset } = useTurnstile({
    endpoint: '',
    onVerify: (token) => console.log('[Hook] token:', token),
    onError: (msg) => console.warn('[Hook] error:', msg),
  })

  const isDisabled = status !== 'idle' && status !== 'error'

  return (
    <div>
      <button
        onClick={verify}
        disabled={isDisabled}
        style={{
          padding: '10px 24px',
          fontSize: 15,
          fontWeight: 600,
          borderRadius: 8,
          border: '2px solid ' + (isDisabled ? '#ccc' : '#6366f1'),
          background: status === 'success' ? '#dcfce7' : isDisabled ? '#f5f5f5' : '#6366f1',
          color: status === 'success' ? '#166534' : isDisabled ? '#999' : '#fff',
          cursor: isDisabled ? 'not-allowed' : 'pointer',
          transition: 'all 0.2s',
        }}
      >
        {status === 'idle' && 'Verify'}
        {status === 'fetching' && 'Fetching…'}
        {status === 'solving' && 'Computing…'}
        {status === 'verifying' && 'Verifying…'}
        {status === 'success' && '✓ Verified'}
        {status === 'error' && 'Retry'}
      </button>
      {status === 'error' && (
        <div style={{ marginTop: 8, padding: '8px 12px', background: '#fef2f2', borderRadius: 6, fontSize: 13, color: '#dc2626' }}>
          {errorMessage}
          {' '}
          <button onClick={reset} style={{ fontSize: 12, cursor: 'pointer', border: 'none', background: 'none', color: '#6366f1', textDecoration: 'underline' }}>
            Reset
          </button>
        </div>
      )}
    </div>
  )
}

export default function App() {
  const mountRef = useRef(null)

  useEffect(() => {
    if (mountRef.current) {
      mount(mountRef.current, {
        endpoint: '',
        onVerify: (token) => console.log('[Mount] token:', token),
      })
    }
  }, [])

  return (
    <div style={{ maxWidth: 720, margin: '40px auto', padding: '0 20px', fontFamily: 'system-ui, sans-serif' }}>
      <h1 style={{ marginBottom: 4 }}>webrify-turnstile</h1>
      <p style={{ color: '#666', marginBottom: 40, marginTop: 0 }}>
        npm demo — three integration patterns
      </p>

      <section style={sectionStyle}>
        <h2 style={{ marginTop: 0 }}>1. Default Component</h2>
        <p style={hintStyle}>&lt;TurnstileWidget&gt; — drop-in with className/style props</p>
        <TurnstileWidget
          endpoint=""
          onVerify={(token) => console.log('[Component] token:', token)}
          style={{ fontSize: 15, padding: '10px 20px' }}
        />
      </section>

      <section style={sectionStyle}>
        <h2 style={{ marginTop: 0 }}>2. Headless Hook</h2>
        <p style={hintStyle}>useTurnstile() — fully custom UI with status/errorMessage/verify/reset</p>
        <CustomCaptcha />
      </section>

      <section style={sectionStyle}>
        <h2 style={{ marginTop: 0 }}>3. Imperative mount()</h2>
        <p style={hintStyle}>mount(container, opts) — for non-React pages</p>
        <div ref={mountRef}></div>
      </section>

      <footer style={{ marginTop: 40, paddingTop: 20, borderTop: '1px solid #eee', fontSize: 12, color: '#aaa', textAlign: 'center' }}>
        webrify-turnstile@0.2.0 · npm demo
      </footer>
    </div>
  )
}
