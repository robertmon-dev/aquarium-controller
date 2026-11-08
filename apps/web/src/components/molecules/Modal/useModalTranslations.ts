import { useT } from "@aquarium/i18n/use-locale";

export const useModalTranslations = () => {
  const t = useT();

  return { closeAria: t("common.close") };
};
