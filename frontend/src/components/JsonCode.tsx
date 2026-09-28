import HljsCode from '@/elements/editors/HljsCode.tsx';
import Code from '@/elements/typography/Code.tsx';

const loadJsonLanguage = () => import('highlight.js/lib/languages/json').then((mod) => mod.default);

// pretty prints JSON text when possible and falls back to the raw text otherwise
export default function JsonCode({ value }: { value: unknown }) {
  let text: string;
  if (typeof value === 'string') {
    try {
      text = JSON.stringify(JSON.parse(value), null, 2);
    } catch {
      return (
        <Code block className='max-h-96 overflow-auto whitespace-pre-wrap break-all'>
          {value}
        </Code>
      );
    }
  } else {
    text = JSON.stringify(value, null, 2);
  }

  return (
    <HljsCode languageName='json' language={loadJsonLanguage} className='max-h-96 overflow-auto'>
      {text}
    </HljsCode>
  );
}
