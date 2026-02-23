import { useState, useCallback } from "react";

interface AuthState {
  isAuthenticated: boolean;
  email: string | null;
  tier: "free" | "paid";
  accessToken: string | null;
}

const initialState: AuthState = {
  isAuthenticated: false,
  email: null,
  tier: "free",
  accessToken: null,
};

export function useAuth() {
  const [auth, setAuth] = useState<AuthState>(initialState);

  const login = useCallback(async (email: string, _authHash: string) => {
    // Will be connected to API in Phase 3 integration
    setAuth({
      isAuthenticated: true,
      email,
      tier: "free",
      accessToken: null,
    });
  }, []);

  const logout = useCallback(() => {
    setAuth(initialState);
  }, []);

  return { ...auth, login, logout };
}
