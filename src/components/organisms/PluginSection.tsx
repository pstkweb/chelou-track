import {
  Check,
  ExternalLink,
  Folder,
  FolderPlus,
  Info,
  Plug,
  RefreshCw,
  Search,
  Trash2,
  X,
} from 'lucide-react';
import Badge from '@/components/atoms/Badge';
import Button from '@/components/atoms/Button';
import IconButton from '@/components/atoms/IconButton';
import SectionTitle from '@/components/atoms/SectionTitle';
import Spinner from '@/components/atoms/Spinner';
import { useAudioEngine } from '@/contexts/AudioEngineContext';
import cn from '@/lib/cn';
import type { PluginInfo } from '@/lib/ipc';

const SMALL = 'h-7.5 px-3 text-[12.5px]';

function MissingNotice({ name, onHide }: { name: string; onHide: () => void }) {
  return (
    <div className="mb-3 flex items-start gap-2.5 rounded-md border border-mix-(--warn)/45 bg-mix-(--warn)/10 px-3 py-2.5 text-[13px] leading-normal [&>svg]:mt-0.5 [&>svg]:flex-none [&>svg]:text-warn">
      <Info size={15} />
      <span className="flex-1">
        Le plugin précédent, <b>{name}</b>, est introuvable. Tu es repassé en son direct : choisis
        un autre plugin ci-dessous si tu veux.
      </span>
      <IconButton onClick={onHide} title="Masquer" aria-label="Masquer" className="size-6.5">
        <X size={13} />
      </IconButton>
    </div>
  );
}

function ActivePlugin({ plugin }: { plugin: PluginInfo | null }) {
  const { openPluginEditor, unloadPlugin } = useAudioEngine();

  return (
    <div className="flex items-center gap-3 rounded-lg border border-border bg-bg2 p-3">
      <span className="type-ic tab size-10">
        <Plug size={19} />
      </span>
      <div className="min-w-0 flex-1">
        <div className="eyebrow mb-0.5 text-[9.5px]">Plugin actif</div>
        <div className="truncate font-bold text-[14.5px]">{plugin ? plugin.name : 'Aucun'}</div>
        <div className="text-fg2 text-xs">
          {plugin
            ? `${plugin.vendor} · VST3`
            : 'Ta guitare est entendue en son direct, sans ampli.'}
        </div>
      </div>
      {plugin && (
        <div className="flex flex-wrap justify-end gap-1.5">
          <Button className="h-8 px-3 text-[12.5px]" onClick={openPluginEditor}>
            <ExternalLink size={14} /> Ouvrir l'interface
          </Button>
          <IconButton
            onClick={unloadPlugin}
            title="Retirer le plugin"
            aria-label="Retirer le plugin"
            className="size-8 border border-border"
          >
            <Trash2 size={15} />
          </IconButton>
        </div>
      )}
    </div>
  );
}

function PluginList({ plugins }: { plugins: PluginInfo[] }) {
  const { status, scanning, loadingPlugin, loadPlugin } = useAudioEngine();

  return (
    <div className={cn(['flex flex-col gap-1', scanning && 'opacity-50'])}>
      {plugins.map((p) => {
        const on = p.id === status?.plugin?.id;
        return (
          <div
            key={p.id}
            className={cn([
              'flex items-center gap-3 rounded-md border border-border px-3 py-2',
              on && 'border-mix-(--accent)/55 bg-mix-(--accent)/7',
            ])}
          >
            <div className="min-w-0 flex-1">
              <div className="truncate font-semibold text-[13.5px]">{p.name}</div>
              <div className="mono truncate text-[10.5px] text-fg3">
                {p.vendor} · {p.file}
              </div>
            </div>
            {on ? (
              <span className="flex items-center gap-1 font-semibold text-accent text-xs">
                <Check size={13} /> Chargé
              </span>
            ) : (
              <Button
                className={SMALL}
                disabled={loadingPlugin !== null}
                onClick={() => loadPlugin(p.id)}
              >
                {loadingPlugin === p.id ? <Spinner /> : 'Charger'}
              </Button>
            )}
          </div>
        );
      })}
    </div>
  );
}

function Folders() {
  const { folders, addFolder, removeFolder } = useAudioEngine();
  const row =
    'flex min-h-7.5 items-center gap-2 text-[12.5px] text-fg2 [&>svg]:flex-none [&>svg]:text-fg3';

  return (
    <>
      <div className="mt-4.5 mb-2 font-semibold text-[12.5px] text-fg2">Dossiers scannés</div>
      <div className="flex flex-col gap-1">
        {folders?.standard.map((f) => (
          <div key={f} className={row}>
            <Folder size={14} />
            <span className="mono min-w-0 flex-1 truncate">{f}</span>
            <Badge>Standard</Badge>
          </div>
        ))}
        {folders?.custom.map((f) => (
          <div key={f} className={row}>
            <Folder size={14} />
            <span className="mono min-w-0 flex-1 truncate">{f}</span>
            <IconButton
              onClick={() => removeFolder(f)}
              title="Retirer ce dossier"
              className="size-6.5"
            >
              <X size={13} />
            </IconButton>
          </div>
        ))}
        <button type="button" className="pop-link mt-1" onClick={addFolder}>
          <FolderPlus size={14} /> Ajouter un dossier…
        </button>
      </div>
    </>
  );
}

function scanLabel(scanning: boolean, plugins: PluginInfo[] | null) {
  if (scanning) {
    return 'Scan en cours…';
  }

  return plugins ? `${plugins.length} VST3 trouvés` : 'Aucun scan effectué';
}

function FirstScan() {
  const { scanning, scanPlugins } = useAudioEngine();

  return (
    <div className="rounded-lg border border-border border-dashed p-4">
      <p className="mt-0 mb-3 text-[13px] text-fg2 leading-normal">
        Chelou Track cherche les plugins VST3 dans les dossiers standard de ton système, plus ceux
        que tu ajoutes.
      </p>
      <Button variant="primary" className="h-9" disabled={scanning} onClick={scanPlugins}>
        {scanning ? <Spinner /> : <Search size={15} />}
        {scanning ? 'Scan en cours…' : 'Scanner les plugins'}
      </Button>
    </div>
  );
}

/** Section « Plugin d'ampli » des réglages : plugin actif, VST3 trouvés, dossiers scannés. */
export default function PluginSection() {
  const { status, settings, scanning, scanPlugins, unloadPlugin } = useAudioEngine();
  if (!status || !settings) {
    return null;
  }
  const plugins = settings.scan;

  return (
    <section>
      <SectionTitle icon={Plug}>Plugin d'ampli</SectionTitle>
      {status.pluginMissing && <MissingNotice name={status.pluginMissing} onHide={unloadPlugin} />}
      <ActivePlugin plugin={status.plugin} />

      <div className="mt-4.5 mb-2 flex items-center justify-between">
        <span className="font-semibold text-[12.5px] text-fg2">{scanLabel(scanning, plugins)}</span>
        {plugins && (
          <Button variant="ghost" className={SMALL} disabled={scanning} onClick={scanPlugins}>
            {scanning ? <Spinner /> : <RefreshCw size={14} />} Rescanner
          </Button>
        )}
      </div>
      {plugins ? <PluginList plugins={plugins} /> : <FirstScan />}
      <Folders />
    </section>
  );
}
