"use client";

import { createContext, useContext, useState } from "react";

type ProviderDialogContextValue = {
  open: boolean;
  setOpen: (open: boolean) => void;
};

const ProviderDialogContext = createContext<ProviderDialogContextValue | null>(null);

export function ProviderDialogProvider({ children }: { children: React.ReactNode }) {
  const [open, setOpen] = useState(false);

  return (
    <ProviderDialogContext.Provider value={{ open, setOpen }}>
      {children}
    </ProviderDialogContext.Provider>
  );
}

export function useProviderDialog() {
  const context = useContext(ProviderDialogContext);
  if (!context) {
    throw new Error("useProviderDialog must be used within ProviderDialogProvider");
  }
  return context;
}
