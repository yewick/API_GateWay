import { useState, type InputHTMLAttributes, type TextareaHTMLAttributes } from "react";
import { Eye, EyeOff } from "lucide-react";

interface BaseProps {
  label?: string;
  error?: string;
  hint?: string;
  containerClassName?: string;
}

export interface InputProps
  extends BaseProps,
    Omit<InputHTMLAttributes<HTMLInputElement>, "size"> {
  textarea?: false;
}

interface TextareaProps
  extends BaseProps,
    Omit<TextareaHTMLAttributes<HTMLTextAreaElement>, "size"> {
  textarea: true;
}

type Props = InputProps | TextareaProps;

const baseField =
  "w-full px-3 py-2 text-sm bg-bg-tertiary border border-border-primary rounded-lg " +
  "text-text-primary placeholder-text-muted outline-none transition-colors " +
  "focus:border-accent focus:ring-1 focus:ring-accent/40";

export function Input(props: Props) {
  const { label, error, hint, containerClassName = "" } = props;

  // 密码可见切换
  const [showPassword, setShowPassword] = useState(false);
  const isPassword = !props.textarea && (props as InputProps).type === "password";
  const effectiveType =
    isPassword && showPassword ? "text" : (props as InputProps).type;

  const renderError = error && (
    <p className="mt-1 text-xs text-danger">{error}</p>
  );
  const renderHint = !error && hint && (
    <p className="mt-1 text-xs text-text-muted">{hint}</p>
  );

  let field: React.ReactNode;
  if (props.textarea) {
    const { label: _l, error: _e, hint: _h, containerClassName: _c, textarea, ...rest } =
      props;
    field = (
      <textarea
        {...(rest as TextareaHTMLAttributes<HTMLTextAreaElement>)}
        className={`${baseField} resize-y min-h-[80px] ${rest.className ?? ""}`}
      />
    );
  } else {
    const {
      label: _l,
      error: _e,
      hint: _h,
      containerClassName: _c,
      textarea: _t,
      type,
      ...rest
    } = props;
    field = (
      <div className="relative">
        <input
          {...(rest as InputHTMLAttributes<HTMLInputElement>)}
          type={effectiveType}
          className={`${baseField} ${
            isPassword ? "pr-10" : ""
          } ${rest.className ?? ""}`}
        />
        {isPassword && (
          <button
            type="button"
            tabIndex={-1}
            onClick={() => setShowPassword((v) => !v)}
            className="absolute inset-y-0 right-0 pr-3 flex items-center text-text-muted hover:text-text-secondary"
            aria-label={showPassword ? "隐藏密码" : "显示密码"}
          >
            {showPassword ? <EyeOff size={16} /> : <Eye size={16} />}
          </button>
        )}
      </div>
    );
  }

  return (
    <div className={containerClassName}>
      {label && (
        <label className="block mb-1.5 text-sm font-medium text-text-secondary">
          {label}
        </label>
      )}
      {field}
      {renderError}
      {renderHint}
    </div>
  );
}
