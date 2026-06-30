import { AlertCircle, Check, Cloud, Guitar, Lock } from "lucide-react";
import { type CSSProperties, useEffect, useState } from "react";
import Button from "@/components/atoms/Button";
import Spinner from "@/components/atoms/Spinner";
import FolderPicker from "@/components/organisms/FolderPicker";
import { useBreadcrumb } from "@/contexts/BreadcrumbContext";
import { pcloudOauthStart } from "@/lib/ipc";

type ConnectScreenProps = {
  startAtFolder?: boolean;
  onConnected: () => void;
};

type ConnectPhase = "idle" | "consent" | "folder";

const OAUTH_SCOPES = [
  "Lister tes dossiers et fichiers",
  "Lire tes méthodes vidéo, PDF et tablatures",
  "Aucune modification ni suppression",
];

export default function ConnectScreen({ startAtFolder = false, onConnected }: ConnectScreenProps) {
  const [phase, setPhase] = useState<ConnectPhase>(startAtFolder ? "folder" : "idle");
  const [oauthErr, setOauthErr] = useState<string | null>(null);
  const { dispatch: dispatchBreadcrumb } = useBreadcrumb();

  useEffect(() => {
    dispatchBreadcrumb({ type: "clear" });
  }, [dispatchBreadcrumb]);

  const startOAuth = async () => {
    setOauthErr(null);
    setPhase("consent"); // show spinner before opening the popup
    try {
      await pcloudOauthStart(); // blocks until OAuth complete or cancelled
      setPhase("folder");
    } catch (e) {
      setOauthErr(String(e));
      setPhase("idle");
    }
  };

  return (
    <div className="relative flex flex-1 overflow-hidden">
      {/* panneau gauche — marque */}
      <div
        className="to[var(--bg-2)] relative flex shrink grow basis-[46%] flex-col justify-between border-r border-r-border border-solid bg-linear-[150deg] from-(--gradient-from) p-[clamp(32px,5vw,64px)]"
        style={
          {
            "--gradient-from": "color-mix(in srgb, var(--accent) 30%, var(--bg-2))",
          } as CSSProperties
        }
      >
        <div className="flex items-center gap-3">
          <div className="flex size-10 items-center justify-center rounded bg-accent text-accentink">
            <Guitar size={24} />
          </div>
          <div className="display text-2xl">Chelou&nbsp;Track</div>
        </div>
        <div className="flex-1 content-center">
          <div className="display mb-5 text-[clamp(38px,4.8vw,66px)]">
            Tes méthodes,
            <br />
            sur ton manche.
          </div>
          <p className="m-0 max-w-105 text-fg2 text-lg leading-[1.55]">
            Branche ton cloud, retrouve tes méthodes vidéo, reprends là où tu t'es arrêté et bosse
            tes tablatures sur les backing tracks. Sans prise de tête.
          </p>
        </div>
      </div>

      {/* panneau droit */}
      <div className="flex shrink grow basis-[54%] items-center justify-center p-8">
        {phase !== "folder" ? (
          <div className="card w-[min(400px,100%)] p-8">
            <div className="mb-5 flex items-center gap-2.5">
              <div className="flex size-8 items-center justify-center rounded-sm bg-chip text-accent">
                <Cloud size={20} />
              </div>
              <div>
                <div className="eyebrow">Étape 1 / 3</div>
                <h2 className="display m-0 text-2xl">Connexion à pCloud</h2>
              </div>
            </div>

            {oauthErr && (
              <div
                role="alert"
                className="mb-4 flex items-start gap-2.5 rounded border-[color-mix(in_srgb,var(--color-red-800)_45%,transparent)] bg-[color-mix(in_srgb,var(--color-red-800)_12%,transparent)] px-3.5 py-3 text-red-800 text-sm/snug"
              >
                <span className="mt-0.5 flex-initial">
                  <AlertCircle size={16} />
                </span>
                <span>{oauthErr}</span>
              </div>
            )}

            <p className="m-0 mb-4 text-fg2 text-sm/relaxed">
              Chelou Track utilise l'authentification sécurisée{" "}
              <strong className="text-fg">OAuth&nbsp;2.0</strong> de pCloud. Tu seras redirigé vers
              pCloud pour autoriser l'accès — ton mot de passe ne transite jamais par l'app.
            </p>

            <div className="m-0 mb-5 flex flex-col gap-2">
              {OAUTH_SCOPES.map((s) => (
                <div key={s} className="flex items-center gap-2.5 text-fg2 text-xs">
                  <span className="flex text-accent">
                    <Check size={15} />
                  </span>
                  {s}
                </div>
              ))}
            </div>

            <Button
              variant="primary"
              size="lg"
              className="w-full"
              onClick={startOAuth}
              disabled={phase === "consent"}
            >
              {phase === "consent" ? (
                <>
                  <Spinner light /> Connexion OAuth en cours...
                </>
              ) : (
                <>
                  <Cloud size={18} /> Continuer avec pCloud
                </>
              )}
            </Button>

            <div className="mt-5 flex items-center gap-2 text-fg3 text-xs">
              <Lock size={14} /> OAuth 2.0 · aucun mot de passe stocké
            </div>
          </div>
        ) : (
          <FolderPicker onConnected={onConnected} />
        )}
      </div>
    </div>
  );
}
