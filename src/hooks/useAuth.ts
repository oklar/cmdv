import { useState, useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

interface AuthState {
  isAuthenticated: boolean;
  email: string | null;
  hasSubscription: boolean;
  loading: boolean;
}

const initialState: AuthState = {
  isAuthenticated: false,
  email: null,
  hasSubscription: false,
  loading: true,
};

export function useAuth() {
  const [auth, setAuth] = useState<AuthState>(initialState);

  const refreshStatus = useCallback(async () => {
    try {
      const status = await invoke<{
        is_authenticated: boolean;
        email: string | null;
        has_subscription: boolean;
      }>("get_auth_status");
      setAuth({
        isAuthenticated: status.is_authenticated,
        email: status.email,
        hasSubscription: status.has_subscription,
        loading: false,
      });
    } catch {
      setAuth({ ...initialState, loading: false });
    }
  }, []);

  useEffect(() => {
    refreshStatus();
  }, [refreshStatus]);

  const login = useCallback(
    async (email: string, password: string) => {
      await invoke("login", { email, password });
      await refreshStatus();
    },
    [refreshStatus]
  );

  const register = useCallback(
    async (email: string, password: string) => {
      await invoke("register", { email, password });
      await refreshStatus();
    },
    [refreshStatus]
  );

  const logout = useCallback(async () => {
    await invoke("logout");
    await refreshStatus();
  }, [refreshStatus]);

  const checkSubscription = useCallback(async () => {
    const active = await invoke<boolean>("check_subscription");
    setAuth((prev) => ({ ...prev, hasSubscription: active }));
    return active;
  }, []);

  return {
    ...auth,
    login,
    register,
    logout,
    checkSubscription,
    refreshStatus,
  };
}
