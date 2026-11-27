// Traductions anglaises, par zone de l'interface (clé = texte français exact).
import common from './common';
import views from './views';
import settings from './settings';
import components from './components';

const en: Record<string, string> = { ...common, ...views, ...settings, ...components };
export default en;
