import { cardStyles, spacing } from "@aquarium/theme";
import { Text, View } from "react-native";
import { Chip } from "@/components/atoms";
import { LOCALES } from "@/i18n";
import { useLocale } from "@/i18n/use-locale";

export function LanguageSwitcher() {
  const { locale, setLocale, t } = useLocale();

  return (
    <View style={{ gap: spacing.xs }}>
      <Text style={cardStyles.sectionLabel}>{t("language.label")}</Text>
      <View style={cardStyles.chipRow}>
        {LOCALES.map((code) => (
          <Chip
            key={code}
            label={t(`language.${code}`)}
            selected={code === locale}
            onPress={() => void setLocale(code)}
          />
        ))}
      </View>
    </View>
  );
}
