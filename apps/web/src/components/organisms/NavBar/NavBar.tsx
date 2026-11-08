import type { NavbarProps } from "./types";
import "./NavBar.scss";

export const Navbar = ({ children, className = "", ...rest }: NavbarProps) => {
  return (
    <header className={`navbar ${className}`.trim()} {...rest}>
      <div className="navbar__container">{children}</div>
    </header>
  );
};
