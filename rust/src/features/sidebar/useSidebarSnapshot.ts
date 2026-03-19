import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import * as api from "@/lib/tauri-api";
import { useSidebarStore } from "@/stores/sidebarStore";
import type { SidebarInvalidationEvent } from "@/lib/types";

export function useSidebarSnapshot() {
  const snapshot = useSidebarStore((s) => s.snapshot);
  const invalidatedVersion = useSidebarStore((s) => s.invalidatedVersion);
  const applySnapshot = useSidebarStore((s) => s.applySnapshot);
  const setSidebarLoading = useSidebarStore((s) => s.setLoading);
  const invalidateSidebar = useSidebarStore((s) => s.invalidate);

  const [snapshotLoading, setSnapshotLoading] = useState(false);

  const fetchSnapshot = useCallback(async () => {
    setSnapshotLoading(true);
    setSidebarLoading(true);
    try {
      const resp = await api.sidebarSnapshotGet({
        chatName: undefined,
        limit: 50,
      });
      if (resp.data) {
        applySnapshot(resp.data);
      }
    } catch {
      // ignore and keep old snapshot
    } finally {
      setSnapshotLoading(false);
      setSidebarLoading(false);
    }
  }, [applySnapshot, setSidebarLoading]);

  useEffect(() => {
    fetchSnapshot();
  }, [fetchSnapshot, invalidatedVersion]);

  useEffect(() => {
    const unlisten = listen<SidebarInvalidationEvent>(
      "sidebar-invalidated",
      (event) => {
        invalidateSidebar(event.payload.version);
      },
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [invalidateSidebar]);

  return {
    snapshot,
    snapshotLoading,
  };
}
