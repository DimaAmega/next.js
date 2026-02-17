// importing externals should work fine
import 'fs'
import 'path'

// importing typescript files should work fine
import './my-plugin'

export default {
  plugins: {
    tailwindcss: {},
    autoprefixer: {},
  },
}
