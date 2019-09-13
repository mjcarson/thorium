const scrollToSection = (value) => {
  // jump if valid id has been provided
  if (document.getElementById(value)) {
    // document.getElementById(value).getBoundingClientRect().top);
    document.getElementById(value).scrollIntoView({ behavior: 'smooth' });
  } else {
    console.log('Error: scroll target does not exist!');
  }
};

export { scrollToSection };
