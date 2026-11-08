import type { FooterProps } from "./types";
import "./Footer.scss";

export const Footer = ({
  brand,
  copyright,
  children,
  className = "",
  ...rest
}: FooterProps) => {
  return (
    <footer className={`app-footer ${className}`.trim()} {...rest}>
      {children ? (
        children
      ) : (
        <div className="app-footer__container">
          {brand && <div className="app-footer__brand">{brand}</div>}
          <nav className="app-footer__nav"></nav>
          {copyright && (
            <div className="app-footer__copyright">{copyright}</div>
          )}
        </div>
      )}
    </footer>
  );
};
