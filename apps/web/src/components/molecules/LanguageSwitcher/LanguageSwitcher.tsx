import { LOCALES } from "@aquarium/i18n";
import { useLocale } from "@aquarium/i18n/use-locale";
import { Button } from "@/components/atoms";

export function LanguageSwitcher() {
  const { locale, setLocale, t } = useLocale();

  return (
    <div className="picker">
      {LOCALES.map((code) => (
        <Button
          key={code}
          variant={code === locale ? "primary" : "outline"}
          onClick={() => void setLocale(code)}
        >
          {t(`language.${code}`)}
        </Button>
      ))}
    </div>
  );
}
