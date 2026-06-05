/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{vue,ts}'],
  theme: {
    extend: {
      fontFamily: {
        sans: ['Inter', '-apple-system', 'BlinkMacSystemFont', 'Segoe UI', 'Microsoft YaHei', 'sans-serif'],
      },
      colors: {
        primary: '#3B82F6',
        gain: '#10B981',
        loss: '#EF4444',
        textMain: '#1F2937',
        textMuted: '#6B7280',
        panel: '#F9FAFB',
        line: '#E5E7EB',
      },
      boxShadow: {
        soft: '0 1px 3px rgba(0,0,0,0.06)',
        popup: '0 2px 10px rgba(0,0,0,0.1)',
      },
    },
  },
  plugins: [],
}
