"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { CheckCircle2Icon, MailIcon } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Spinner } from "@/components/ui/spinner";
import { acceptOrganizationInvitation, getLoginUrl, getMe } from "@/lib/api";
import type { Organization } from "@/lib/dashboard-types";

const inviteTokenStorageKey = "bella_invite_token";

function tokenFromHash() {
  const hash = window.location.hash.startsWith("#")
    ? window.location.hash.slice(1)
    : window.location.hash;
  return new URLSearchParams(hash).get("token");
}

export default function InvitePage() {
  const router = useRouter();
  const [checkingAuth, setCheckingAuth] = useState(true);
  const [accepting, setAccepting] = useState(false);
  const [authenticated, setAuthenticated] = useState(false);
  const [needsEmailRefresh, setNeedsEmailRefresh] = useState(false);
  const [organization, setOrganization] = useState<Organization>();
  const [token, setToken] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    const load = async () => {
      const hashToken = tokenFromHash();
      const nextToken = hashToken ?? window.sessionStorage.getItem(inviteTokenStorageKey) ?? "";
      if (nextToken) {
        window.sessionStorage.setItem(inviteTokenStorageKey, nextToken);
        setToken(nextToken);
        if (hashToken) {
          window.history.replaceState(null, "", "/invite");
        }
      } else {
        setError("Invitation token is missing.");
      }
      const user = await getMe();
      setAuthenticated(Boolean(user));
      setNeedsEmailRefresh(Boolean(user && !user.primary_email));
      setCheckingAuth(false);
    };
    void load();
  }, []);

  const acceptInvite = async () => {
    if (!token) return;
    setAccepting(true);
    setError("");
    try {
      const nextOrganization = await acceptOrganizationInvitation(token);
      window.sessionStorage.removeItem(inviteTokenStorageKey);
      setOrganization(nextOrganization);
      router.refresh();
    } catch (e) {
      const message = e instanceof Error ? e.message : "Could not accept the invitation.";
      if (message.includes("verified primary email")) {
        setNeedsEmailRefresh(true);
        setError(
          "Bella needs to refresh your verified GitHub email before accepting this invitation.",
        );
      } else {
        setError(message);
      }
    } finally {
      setAccepting(false);
    }
  };

  const login = () => {
    if (token) {
      window.sessionStorage.setItem(inviteTokenStorageKey, token);
    }
    window.location.assign(getLoginUrl(`${window.location.origin}/invite`));
  };

  return (
    <main className="bg-muted flex min-h-svh items-center justify-center p-6">
      <Card className="w-full max-w-md">
        <CardHeader className="text-center">
          <CardTitle className="flex items-center justify-center gap-2 text-xl">
            {organization ? <CheckCircle2Icon /> : <MailIcon />}
            Organization invitation
          </CardTitle>
          <CardDescription>
            Join the Bella organization you were invited to with your verified GitHub email.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          {checkingAuth ? (
            <div className="text-muted-foreground flex items-center justify-center gap-2 text-sm">
              <Spinner />
              Checking session
            </div>
          ) : organization ? (
            <>
              <Alert>
                <AlertDescription>
                  You joined {organization.name} as {organization.role}.
                </AlertDescription>
              </Alert>
              <Button asChild>
                <Link href="/settings/organization">Open organization settings</Link>
              </Button>
            </>
          ) : authenticated && needsEmailRefresh ? (
            <>
              <Alert>
                <AlertDescription>
                  Bella needs to refresh your verified GitHub email before accepting this
                  invitation.
                </AlertDescription>
              </Alert>
              {error && (
                <Alert variant="destructive">
                  <AlertDescription>{error}</AlertDescription>
                </Alert>
              )}
              <Button onClick={login} disabled={!token}>
                Reconnect GitHub
              </Button>
            </>
          ) : authenticated ? (
            <>
              {error && (
                <Alert variant="destructive">
                  <AlertDescription>{error}</AlertDescription>
                </Alert>
              )}
              <Button onClick={() => void acceptInvite()} disabled={accepting || !token}>
                {accepting && <Spinner data-icon="inline-start" />}
                Accept invitation
              </Button>
            </>
          ) : (
            <>
              {error && (
                <Alert variant="destructive">
                  <AlertDescription>{error}</AlertDescription>
                </Alert>
              )}
              <Button onClick={login} disabled={!token}>
                Continue with GitHub
              </Button>
            </>
          )}
        </CardContent>
      </Card>
    </main>
  );
}
