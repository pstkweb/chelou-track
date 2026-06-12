import { AlertCircle, Cloud, Guitar, Lock } from "lucide-react";
import Button from "../atoms/Button";
import Field from "../atoms/Field";
import Spinner from "../atoms/Spinner";
import { useState } from "react";
import FolderPicker from "../organisms/FolderPicker";
import { pcloudLogin } from "../../lib/ipc";
import cn from "../../lib/cn";

type ConnectScreenProps = {
    onConnected: () => void;
};
type ConnectPhase = 'login' | 'connecting' | 'folder';

type ConnectErrors = {
    email?: string;
    password?: string;
} | null;

const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

export default function ConnectScreen({ onConnected }: ConnectScreenProps) {
    const [email, setEmail] = useState('');
    const [pwd, setPwd] = useState('');
    const [state, setState] = useState<ConnectPhase>('login');
    const [errors, setErrors] = useState<ConnectErrors>(null);
    const [globalError, setGlobalError] = useState<string | null>(null);

    const handleEmailChange = (e: React.ChangeEvent<HTMLInputElement>) => {
        setEmail(e.target.value);

        if (errors?.email) {
            setErrors(prev => {
                if (!prev) return null;

                const { email, ...rest } = prev;

                return Object.keys(rest).length > 0 ? rest : null;
            });
        }

        if (globalError) {
            setGlobalError(null);
        }
    };

    const handlePasswordChange = (e: React.ChangeEvent<HTMLInputElement>) => {
        setPwd(e.target.value);

        if (errors?.password) {
            setErrors(prev => {
                if (!prev) return null;

                const { password, ...rest } = prev;

                return Object.keys(rest).length > 0 ? rest : null;
            });
        }

        if (globalError) {
            setGlobalError(null);
        }
    };

    const handleSubmit = async (e: React.SubmitEvent<HTMLFormElement>) => {
        e.preventDefault();

        const errors: ConnectErrors = {};

        if (!email.trim()) {
            errors.email = 'L\'adresse email est requise.';
        } else if (!EMAIL_RE.test(email)) {
            errors.email = 'Format d\'adresse email invalide.';
        }

        if (!pwd) {
            errors.password = 'Le mot de passe est requis.';
        }

        setErrors(Object.keys(errors).length > 0 ? errors : null);
        setGlobalError(null);

        if (Object.keys(errors).length === 0) {
            setState('connecting');

            try {
                await pcloudLogin(email, pwd);

                setState('folder');
            } catch (error) {
                setState('login');
                setGlobalError(error instanceof Error ? error.message : 'Impossible de vous connecter à pCloud, verifiez vos identifiants et reessayez.');
            }
        }
    };

    return (
        <div className="flex flex-1 overflow-hidden relative">
            {/* panneau gauche — marque */}
            <div className="flex flex-col grow shrink basis-[46%] relative justify-between p-[clamp(32px,5vw,64px)] border-r border-solid border-r-border bg-linear-[150deg] from-[var(--gradient-from)] to[var(--bg-2)]" style={{
                '--gradient-from': 'color-mix(in srgb, var(--accent) 30%, var(--bg-2))',
            }}>
                <div className="flex gap-3 items-center">
                    <div className="flex items-center justify-center size-10 bg-accent text-accentink rounded">
                        <Guitar size={24} />
                    </div>
                    <div className="display text-2xl">Chelou&nbsp;Track</div>
                </div>
                <div className="flex-1 content-center">
                    <div className="display mb-5 text-[clamp(38px,4.8vw,66px)]">
                        Tes méthodes,<br />sur ton manche.
                    </div>
                    <p className="m-0 max-w-105 text-lg leading-[1.55] text-fg2">
                        Branche ton cloud, retrouve tes méthodes vidéo, reprends là où tu t'es arrêté
                        et bosse tes tablatures sur les backing tracks. Sans prise de tête.
                    </p>
                </div>
            </div>

            {/* panneau droit */}
            <div className="flex grow shrink basis-[54%] items-center justify-center p-8">
                {state !== 'folder' ? (
                    <form onSubmit={handleSubmit} noValidate className="bg-surface rounded-lg border border-border relative p-8 w-[min(400px,100%)]">
                        <div className="flex items-center gap-3 mb-5">
                            <div className="flex items-center justify-center size-8 bg-chip text-accent rounded-sm">
                                <Cloud size={20} />
                            </div>
                            <div>
                                <div className="text-xs font-bold tracking-wide uppercase text-fg3">Étape 1 / 2</div>
                                <h2 className="display m-0 text-2xl">Connexion à pCloud</h2>
                            </div>
                        </div>

                        {globalError && (
                            <div role="alert" className="flex items-start gap-2 mb-4 py-2.5 px-3 rounded text-sm text-red-800">
                                <span className="flex-initial mt-0.5"><AlertCircle size={16} /></span>
                                <span>{globalError}</span>
                            </div>
                        )}

                        <label className="block text-sm font-semibold text-fg2 mb-1.5">Identifiant ou e-mail</label>
                        <Field className={cn("pl-3 mb-1.5", errors?.password && "border-red-800")} type="text" placeholder="toi@exemple.com"
                            value={email} onChange={handleEmailChange} disabled={state === 'connecting'} autoFocus aria-invalid={!!errors?.email} />
                        {errors?.email && (
                            <div className="flex items-center gap-1.5 mb-3.5 text-xs text-red-800">
                                <AlertCircle size={13} /> {errors.email}
                            </div>
                        )}

                        <label className="block text-sm font-semibold text-fg2 mb-1.5">Mot de passe</label>
                        <Field className={cn("pl-3 mb-5", errors?.password && "border-red-800 mb-1.5")} type="password" placeholder="••••••••"
                            value={pwd} onChange={handlePasswordChange} disabled={state === 'connecting'} aria-invalid={!!errors?.password} />
                        {errors?.password && (
                            <div className="flex items-center gap-1.5 mb-3.5 text-xs text-red-800">
                                <AlertCircle size={13} /> {errors.password}
                            </div>
                        )}

                        <Button type="submit" variant="primary" size="lg" className="w-full" disabled={state === 'connecting'}>
                            {state === 'connecting' ? <><Spinner light /> Connexion…</> : 'Se connecter'}
                        </Button>
                        <div className="flex items-center gap-2 mt-3 text-xs text-fg3">
                            <Lock size={14} /> Tes identifiants restent sur ton appareil
                        </div>
                    </form>
                ) : (
                    <FolderPicker onConnected={onConnected} />
                )}
            </div>
        </div>
    );
}