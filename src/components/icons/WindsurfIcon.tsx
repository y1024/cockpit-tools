import { CSSProperties } from 'react';
import windsurfIcon from '../../assets/icons/windsurf.svg';

type WindsurfIconProps = {
  className?: string;
  style?: CSSProperties;
};

export function WindsurfIcon({ className = 'nav-item-icon', style }: WindsurfIconProps) {
  return (
    <img
      src={windsurfIcon}
      className={className}
      style={style}
      alt=""
      aria-hidden="true"
    />
  );
}
