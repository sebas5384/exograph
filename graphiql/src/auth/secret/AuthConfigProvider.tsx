import React, { useContext, useEffect, useState } from "react";
import { AuthContext } from "../../AuthContext";
import { SecretConfig } from "./SecretConfig";
import * as jose from "jose";
import { SecretAuthContext, SecretAuthProvider } from "./SecretAuthProvider";

type AuthConfig = {
  config: SecretConfig;
  setConfig: (config: SecretConfig) => void;
};

export const AuthConfigContext = React.createContext<AuthConfig>(
  {} as AuthConfig
);

export function AuthConfigProvider(props: { children: React.ReactNode }) {
  const [config, setConfig] = useState(SecretConfig.loadConfig());

  return (
    <AuthConfigContext.Provider
      value={{
        config,
        setConfig,
      }}
    >
      <SecretAuthProvider>
        <ContextInitializer>{props.children}</ContextInitializer>
      </SecretAuthProvider>
    </AuthConfigContext.Provider>
  );
}

function ContextInitializer(props: { children: React.ReactNode }) {
  const { config } = useContext(AuthConfigContext);
  const { signedIn, setSignedIn } = useContext(SecretAuthContext);
  const { setTokenFn, setIsSignedIn, setUserInfo, setSignOutFn } =
    useContext(AuthContext);

  useEffect(() => {
    const secret = config.secret;
    const payload = config.payload;

    setTokenFn &&
      setTokenFn(
        signedIn
          ? () => Promise.resolve(createJwtToken(JSON.parse(payload), secret))
          : undefined
      );
    setIsSignedIn && setIsSignedIn(signedIn);
    setUserInfo && setUserInfo(payload);
    setSignOutFn &&
      setSignOutFn(() => {
        setSignedIn(!signedIn);
        return Promise.resolve();
      });
  }, [
    config,
    setTokenFn,
    setIsSignedIn,
    setUserInfo,
    setSignOutFn,
    setSignedIn,
    signedIn,
  ]);

  return <>{props.children}</>;
}

async function createJwtToken(
  payload: Record<string, unknown>,
  secret: string
): Promise<string | null> {
  if (secret === "") {
    return null;
  }

  const encodedSecret = new TextEncoder().encode(secret);
  const alg = "HS256";

  return await new jose.SignJWT(payload)
    .setProtectedHeader({ alg })
    .setIssuedAt()
    .setExpirationTime("10m")
    .sign(encodedSecret);
}
