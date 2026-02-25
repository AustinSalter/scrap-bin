import { useEffect } from 'react';
import { useAppStore } from '../stores/appStore';

const AUTO_DISMISS_MS = 5000;

export function ErrorToast() {
  const error = useAppStore((s) => s.error);
  const clearError = useAppStore((s) => s.clearError);

  useEffect(() => {
    if (!error) return;
    const timer = setTimeout(clearError, AUTO_DISMISS_MS);
    return () => clearTimeout(timer);
  }, [error, clearError]);

  if (!error) return null;

  return (
    <div className="error-toast" role="alert">
      <span className="error-toast-msg">{error}</span>
      <button className="error-toast-close" onClick={clearError}>
        &times;
      </button>
    </div>
  );
}
