interface ButtonProps {
  disabled?: boolean;
  type?: "submit" | "button" | "reset";
  onClick?: () => void;
  children?: React.ReactNode;
}

export default function Button({
  disabled,
  type,
  onClick,
  children,
}: ButtonProps) {
  return (
    <button
      type={type}
      disabled={disabled}
      onClick={onClick}
      className="text-dark-red bg-green hover:text-orange rounded-md px-2 py-1 font-bold disabled:text-gray-600 disabled:opacity-70"
    >
      {children}
    </button>
  );
}
